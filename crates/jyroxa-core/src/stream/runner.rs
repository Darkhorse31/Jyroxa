//! Parallel segment fetch for HLS and DASH, then merge to a single file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures::future::join_all;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::download::Control;
use crate::error::{Error, Result};
use crate::events::{progress_tick, EventSender, ProgressEvent};
use crate::stream::dash::{dash_segment_urls, parse_mpd};
use crate::stream::hls::{parse_m3u8, HlsPlaylist};
use crate::{build_client, MAX_CONNECTIONS, USER_AGENT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamDownloadResult {
    pub total_size: u64,
    pub output: PathBuf,
}

pub fn is_stream_manifest_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains(".m3u8") || lower.contains(".mpd")
}

fn apply_headers(
    builder: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
) -> reqwest::RequestBuilder {
    let mut b = builder;
    for (k, v) in headers {
        b = b.header(k.as_str(), v.as_str());
    }
    b
}

async fn fetch_text(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<String> {
    let req = apply_headers(client.get(url).header("User-Agent", USER_AGENT), headers);
    let res = req.send().await.map_err(Error::from)?;
    if !res.status().is_success() {
        return Err(Error::other(format!("http {} for {}", res.status(), url)));
    }
    res.text().await.map_err(Error::from)
}

async fn resolve_hls_segments(
    client: &reqwest::Client,
    manifest_url: &str,
    headers: &HashMap<String, String>,
) -> Result<Vec<String>> {
    let text = fetch_text(client, manifest_url, headers).await?;
    let playlist = parse_m3u8(&text, manifest_url).map_err(Error::other)?;
    match playlist {
        HlsPlaylist::Master { variants } => {
            let best = variants
                .iter()
                .max_by_key(|v| v.bandwidth.unwrap_or(0))
                .ok_or_else(|| Error::other("empty HLS master"))?;
            Box::pin(resolve_hls_segments(client, &best.uri, headers)).await
        }
        HlsPlaylist::Media {
            segments,
            init_map,
            ..
        } => {
            let mut urls = Vec::new();
            if let Some(map) = init_map {
                urls.push(map);
            }
            urls.extend(segments.into_iter().map(|s| s.uri));
            Ok(urls)
        }
    }
}

async fn resolve_dash_segments(
    client: &reqwest::Client,
    manifest_url: &str,
    headers: &HashMap<String, String>,
) -> Result<Vec<String>> {
    let text = fetch_text(client, manifest_url, headers).await?;
    let info = parse_mpd(&text, manifest_url).map_err(Error::other)?;
    let best = info
        .variants
        .iter()
        .max_by_key(|v| v.bandwidth.unwrap_or(0))
        .ok_or_else(|| Error::other("empty DASH MPD"))?;
    let mut urls = Vec::new();
    if let Some(init) = best.init_url.clone() {
        urls.push(init);
    }
    urls.extend(dash_segment_urls(best));
    Ok(urls)
}

async fn download_one(
    client: reqwest::Client,
    url: String,
    headers: HashMap<String, String>,
    ctrl: Control,
) -> Result<Vec<u8>> {
    while ctrl.is_paused() {
        if ctrl.is_cancelled() {
            return Err(Error::Cancelled);
        }
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    }
    if ctrl.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let req = apply_headers(client.get(&url).header("User-Agent", USER_AGENT), &headers);
    let res = req.send().await.map_err(Error::from)?;
    if !res.status().is_success() {
        return Err(Error::other(format!("segment http {}", res.status())));
    }
    let bytes = res.bytes().await.map_err(Error::from)?;
    Ok(bytes.to_vec())
}

/// Download an HLS or DASH manifest with parallel segment workers.
pub async fn stream_download(
    manifest_url: &str,
    output: &Path,
    headers: HashMap<String, String>,
    connections: usize,
    events: Option<EventSender>,
    control: Control,
) -> Result<StreamDownloadResult> {
    let client = build_client()?;
    let lower = manifest_url.to_ascii_lowercase();
    let segments = if lower.contains(".mpd") {
        resolve_dash_segments(&client, manifest_url, &headers).await?
    } else {
        resolve_hls_segments(&client, manifest_url, &headers).await?
    };
    if segments.is_empty() {
        return Err(Error::other("no segments resolved"));
    }

    let total_hint: u64 = 0;
    let connections = connections.clamp(1, MAX_CONNECTIONS);
  if let Some(ref tx) = events {
        let _ = tx.send(ProgressEvent::Started {
            total_size: None,
            connections,
            resumed_from: 0,
            supports_ranges: false,
        });
    }

    let downloaded = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let sem = Arc::new(Semaphore::new(connections));
    let mut handles = Vec::with_capacity(segments.len());
    let results: Arc<tokio::sync::Mutex<Vec<(usize, Vec<u8>)>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    for (idx, url) in segments.into_iter().enumerate() {
        let permit = sem.clone().acquire_owned().await.map_err(|_| Error::Cancelled)?;
        let client = client.clone();
        let hdrs = headers.clone();
        let ctrl = control.clone();
        let dl = Arc::clone(&downloaded);
        let bucket = Arc::clone(&results);
        let ev = events.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let data = download_one(client, url, hdrs, ctrl).await?;
            let nbytes = data.len() as u64;
            dl.fetch_add(nbytes, Ordering::Relaxed);
            bucket.lock().await.push((idx, data));
            if let Some(ref tx) = ev {
                let so_far = dl.load(Ordering::Relaxed);
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                let speed = so_far as f64 / elapsed;
                let _ = tx.send(progress_tick(
                    so_far,
                    None,
                    speed,
                    connections,
                    None,
                    Some(speed),
                    Some(elapsed),
                    Some(connections),
                    Some("downloading stream"),
                    Some(false),
                    None,
                    None,
                    None,
                ));
            }
            Ok::<(), Error>(())
        }));
    }

    let joined = join_all(handles).await;
    for j in joined {
        j.map_err(|e| Error::other(e.to_string()))??;
    }

    let mut parts = results.lock().await.clone();
    parts.sort_by_key(|(i, _)| *i);
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let first_bytes: Vec<&[u8]> = parts.iter().map(|(_, b)| b.as_slice()).collect();

    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(Error::from)?;
    }
    let mut file = tokio::fs::File::create(output).await.map_err(Error::from)?;
    let mut total: u64 = 0;
    for (_, data) in &parts {
        file.write_all(data).await.map_err(Error::from)?;
        total += data.len() as u64;
    }
    file.flush().await.map_err(Error::from)?;

    let out_path = output.to_path_buf();
    let needs_remux = out_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp4"))
        .unwrap_or(true);

    let final_path = if needs_remux && is_raw_stream(&out_path, &first_bytes) {
        remux_to_mp4(&out_path, control).await.unwrap_or(out_path)
    } else {
        out_path
    };

    if let Some(ref tx) = events {
        let _ = tx.send(progress_tick(
            total,
            Some(total.max(total_hint)),
            0.0,
            connections,
            None,
            None,
            None,
            Some(0),
            Some("completed"),
            Some(false),
            None,
            None,
            None,
        ));
    }

    Ok(StreamDownloadResult {
        total_size: total,
        output: final_path,
    })
}

fn is_raw_stream(output: &Path, parts: &[&[u8]]) -> bool {
    if output.extension().and_then(|e| e.to_str()) == Some("mp4") {
        return false;
    }
    parts.first().map(|p| p.starts_with(b"\x47")).unwrap_or(false)
}

async fn remux_to_mp4(input: &Path, control: Control) -> Result<PathBuf> {
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mp4 = input.with_extension("mp4");
    let ffmpeg = resolve_ffmpeg();
    if !ffmpeg.exists() {
        // Fall back: rename if already looks like fmp4 concat
        if tokio::fs::rename(input, &mp4).await.is_ok() {
            return Ok(mp4);
        }
        return Ok(input.to_path_buf());
    }
    let status = tokio::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-i",
            &input.to_string_lossy(),
            "-c",
            "copy",
            "-bsf:a",
            "aac_adtstoasc",
            &mp4.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(Error::from)?;
    if status.status.success() {
        let _ = tokio::fs::remove_file(input).await;
        return Ok(mp4);
    }
    Ok(input.to_path_buf())
}

pub fn resolve_ffmpeg() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let side = exe.parent().map(|p| p.join("ffmpeg.exe"));
        if let Some(ref p) = side {
            if p.is_file() {
                return p.clone();
            }
        }
    }
    PathBuf::from("ffmpeg.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_manifest_urls() {
        assert!(is_stream_manifest_url("https://x/y/playlist.m3u8"));
        assert!(is_stream_manifest_url("https://x/manifest.mpd"));
        assert!(!is_stream_manifest_url("https://x/y/video.mp4"));
    }
}