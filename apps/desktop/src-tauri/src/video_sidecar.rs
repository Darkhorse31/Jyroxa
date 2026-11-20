//! yt-dlp sidecar for page-URL video resolution.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct YtFormat {
    pub format_id: String,
    pub ext: Option<String>,
    pub height: Option<u64>,
    pub width: Option<u64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub tbr: Option<f64>,
    pub url: Option<String>,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YtInfo {
    pub id: Option<String>,
    pub title: Option<String>,
    pub webpage_url: Option<String>,
    pub formats: Option<Vec<YtFormat>>,
}

pub fn yt_dlp_path() -> PathBuf {
    crate::sidecar_paths::sidecar_exe("yt-dlp.exe")
}

pub fn ffmpeg_path() -> PathBuf {
    crate::sidecar_paths::sidecar_exe("ffmpeg.exe")
}

pub fn yt_dlp_available() -> bool {
    yt_dlp_path().is_file()
}

pub async fn probe_page_formats(page_url: &str) -> Result<YtInfo, String> {
    let ytdlp = yt_dlp_path();
    if !ytdlp.is_file() {
        return Err("yt-dlp.exe not bundled".into());
    }
    let out = tokio::process::Command::new(&ytdlp)
        .args([
            "--no-warnings",
            "--no-playlist",
            "-J",
            "--flat-playlist",
            page_url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.trim().to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

pub fn pick_format<'a>(formats: &'a [YtFormat], format_id: Option<&str>) -> Option<&'a YtFormat> {
    if let Some(id) = format_id {
        return formats.iter().find(|f| f.format_id == id);
    }
    formats
        .iter()
        .filter(|f| f.vcodec.as_deref().unwrap_or("none") != "none" && f.url.is_some())
        .max_by_key(|f| f.height.unwrap_or(0))
        .or_else(|| formats.iter().find(|f| f.url.is_some()))
}

pub async fn resolve_page_to_url(
    page_url: &str,
    format_id: Option<&str>,
) -> Result<(String, String), String> {
    let info = probe_page_formats(page_url).await?;
    let title = info.title.clone().unwrap_or_else(|| "video".into());
    let formats = info.formats.unwrap_or_default();
    let fmt = pick_format(&formats, format_id).ok_or_else(|| "no matching format".to_string())?;
    let url = fmt
        .url
        .clone()
        .ok_or_else(|| "format has no direct url".to_string())?;
    let ext = fmt.ext.clone().unwrap_or_else(|| "mp4".into());
    let filename = sanitize_filename(&format!("{title}.{ext}"));
    Ok((url, filename))
}

fn sanitize_filename(name: &str) -> String {
    let bad = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    name.chars()
        .map(|c| if bad.contains(&c) { '_' } else { c })
        .collect()
}

pub async fn download_via_ytdlp(
    page_url: &str,
    output: &Path,
    format_id: Option<&str>,
) -> Result<u64, String> {
    let ytdlp = yt_dlp_path();
    if !ytdlp.is_file() {
        return Err("yt-dlp.exe not bundled".into());
    }
    let mut args = vec![
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
        "-o".to_string(),
        output.to_string_lossy().into_owned(),
    ];
    if let Some(id) = format_id {
        args.push("-f".to_string());
        args.push(id.to_string());
    }
    args.push(page_url.to_string());
    let out = tokio::process::Command::new(&ytdlp)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let meta = tokio::fs::metadata(output)
        .await
        .map_err(|e| e.to_string())?;
    Ok(meta.len())
}
