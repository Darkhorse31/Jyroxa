//! yt-dlp sidecar for page-URL video resolution.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct YtFormat {
    pub format_id: String,
    pub ext: Option<String>,
    pub height: Option<u64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub tbr: Option<f64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YtInfo {
    pub title: Option<String>,
    pub formats: Option<Vec<YtFormat>>,
}

pub fn yt_dlp_path() -> PathBuf {
    crate::sidecar_paths::sidecar_exe("yt-dlp.exe")
}

pub fn ffmpeg_path() -> PathBuf {
    crate::sidecar_paths::sidecar_exe("ffmpeg.exe")
}

fn browser_client_args(page_url: &str) -> &'static [&'static str] {
    let Ok(url) = url::Url::parse(page_url) else {
        return &[];
    };
    // Use a browser client for extraction. Leave full media transfers on the
    // default downloader; forcing impersonation there can throttle the CDN.
    if url.host_str().is_some_and(|host| {
        host == "dailymotion.com" || host.ends_with(".dailymotion.com") || host == "dai.ly"
    }) {
        &["--impersonate", "firefox"]
    } else {
        &[]
    }
}

async fn output_ytdlp(
    cmd: &mut tokio::process::Command,
    page_url: &str,
) -> std::io::Result<std::process::Output> {
    for attempt in 0..3 {
        let output = crate::process_spawn::output_tokio(cmd).await?;
        let error = String::from_utf8_lossy(&output.stderr);
        // A fresh extraction obtains new signed CDN URLs. Retrying individual
        // fragments against a rejected URL cannot recover an expired response.
        let refresh = !browser_client_args(page_url).is_empty()
            && ["HTTP Error 403", "HTTP Error 429", "No video formats found"]
                .iter()
                .any(|message| error.contains(message));
        if output.status.success() || !refresh || attempt == 2 {
            return Ok(output);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
    }
    unreachable!("the last attempt always returns its output")
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoFormatChoice {
    pub format_id: String,
    pub label: String,
    pub size_bytes: Option<u64>,
}

fn vcodec_none(fmt: &YtFormat) -> bool {
    fmt.vcodec.as_deref().unwrap_or("none") == "none"
}

fn acodec_none(fmt: &YtFormat) -> bool {
    fmt.acodec.as_deref().unwrap_or("none") == "none"
}

fn format_human_label(fmt: &YtFormat) -> String {
    if vcodec_none(fmt) && !acodec_none(fmt) {
        let ext = fmt.ext.as_deref().unwrap_or("audio");
        return format!("Audio only ({ext})");
    }
    let mut parts = Vec::new();
    if let Some(h) = fmt.height {
        parts.push(format!("{h}p"));
    }
    if let Some(ext) = &fmt.ext {
        parts.push(ext.to_ascii_uppercase());
    }
    if parts.is_empty() {
        format!("Format {}", fmt.format_id)
    } else {
        parts.join(" ")
    }
}

/// Pick a short list of qualities for the in-page video menu.
pub fn formats_for_menu(info: &YtInfo) -> Vec<VideoFormatChoice> {
    let formats = info.formats.as_deref().unwrap_or_default();
    let mut out = Vec::new();
    let mut seen_heights = std::collections::HashSet::new();

    let mut progressive: Vec<&YtFormat> = formats
        .iter()
        .filter(|f| !vcodec_none(f) && !acodec_none(f) && f.url.is_some())
        .collect();
    progressive.sort_by_key(|f| std::cmp::Reverse(f.height.unwrap_or(0)));

    for fmt in progressive {
        let h = fmt.height.unwrap_or(0);
        if h > 0 && !seen_heights.insert(h) {
            continue;
        }
        out.push(VideoFormatChoice {
            format_id: fmt.format_id.clone(),
            label: format_human_label(fmt),
            size_bytes: fmt.filesize,
        });
        if out.len() >= 8 {
            return out;
        }
    }

    let audio = formats
        .iter()
        .filter(|f| vcodec_none(f) && !acodec_none(f))
        .max_by_key(|f| f.tbr.unwrap_or(0.0) as u64);
    if let Some(a) = audio {
        out.push(VideoFormatChoice {
            format_id: a.format_id.clone(),
            label: "Audio only".into(),
            size_bytes: a.filesize,
        });
    }

    for height in [1080u64, 720, 480, 360, 240] {
        let video = formats
            .iter()
            .filter(|f| !vcodec_none(f) && f.height == Some(height))
            .max_by_key(|f| f.tbr.unwrap_or(0.0) as u64);
        let audio = formats
            .iter()
            .filter(|f| vcodec_none(f) && !acodec_none(f))
            .max_by_key(|f| f.tbr.unwrap_or(0.0) as u64);
        let (Some(v), Some(a)) = (video, audio) else {
            continue;
        };
        let fid = format!("{}+{}", v.format_id, a.format_id);
        if out.iter().any(|c| c.format_id == fid) {
            continue;
        }
        out.push(VideoFormatChoice {
            format_id: fid,
            label: format!("{height}p (merged)"),
            size_bytes: v.filesize.or(a.filesize),
        });
        if out.len() >= 12 {
            break;
        }
    }

    if out.is_empty() {
        out.push(VideoFormatChoice {
            format_id: "bv*+ba/b".into(),
            label: "Best available".into(),
            size_bytes: None,
        });
    }
    out
}

pub async fn probe_page_formats(page_url: &str) -> Result<YtInfo, String> {
    let ytdlp = yt_dlp_path();
    if !ytdlp.is_file() {
        return Err("yt-dlp.exe not bundled".into());
    }
    let mut cmd = tokio::process::Command::new(&ytdlp);
    cmd.args(browser_client_args(page_url))
        .args([
            "--no-warnings",
            "--no-playlist",
            "-J",
            "--flat-playlist",
            page_url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = output_ytdlp(&mut cmd, page_url)
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

pub fn sanitize_filename_public(name: &str) -> String {
    sanitize_filename(name)
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
    let ffmpeg = ffmpeg_path();
    let mut args = vec![
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
        // A completed file must contain every fragment. On a rejected CDN URL,
        // fail this attempt so output_ytdlp can refresh the extraction instead
        // of silently skipping fragments for the rest of the video.
        "--abort-on-unavailable-fragments".to_string(),
    ];
    if ffmpeg.is_file() {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg.to_string_lossy().into_owned());
        args.push("--merge-output-format".into());
        args.push("mp4".into());
    }
    let out_arg = if output.extension().is_some() {
        output.to_string_lossy().into_owned()
    } else {
        format!("{}.%(ext)s", output.display())
    };
    args.push("-o".to_string());
    args.push(out_arg);
    if let Some(id) = format_id.filter(|s| !s.is_empty()) {
        args.push("-f".to_string());
        args.push(id.to_string());
    } else {
        args.push("-f".to_string());
        args.push("bv*+ba/b".to_string());
    }
    args.push(page_url.to_string());
    let mut cmd = tokio::process::Command::new(&ytdlp);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = output_ytdlp(&mut cmd, page_url)
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    if let Ok(m) = tokio::fs::metadata(output).await {
        return Ok(m.len());
    }
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let parent = output.parent().unwrap_or(Path::new("."));
    let mut rd = tokio::fs::read_dir(parent)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(ent) = rd.next_entry().await.map_err(|e| e.to_string())? {
        let fname = ent.file_name();
        let name = fname.to_string_lossy();
        if name.starts_with(stem) && name.len() > stem.len() {
            let m = ent.metadata().await.map_err(|e| e.to_string())?;
            return Ok(m.len());
        }
    }
    Err("yt-dlp output file missing".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn rejected_cdn_urls_refresh_with_bounded_hidden_processes() {
        let directory = tempfile::tempdir().unwrap();
        for (name, url, failures, error, expected_attempts, success) in [
            (
                "recover",
                "https://dai.ly/test",
                1,
                "HTTP Error 403",
                2,
                true,
            ),
            (
                "bounded",
                "https://dai.ly/test",
                9,
                "HTTP Error 403",
                3,
                false,
            ),
            (
                "permanent",
                "https://dai.ly/test",
                9,
                "Video removed",
                1,
                false,
            ),
            (
                "other",
                "https://youtube.com/test",
                9,
                "HTTP Error 403",
                1,
                false,
            ),
        ] {
            let count_path = directory.path().join(name);
            let mut cmd = tokio::process::Command::new("powershell.exe");
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                r#"
                $path = $env:JYROXA_TEST_ATTEMPTS
                $count = 1
                if ([IO.File]::Exists($path)) { $count += [int][IO.File]::ReadAllText($path) }
                [IO.File]::WriteAllText($path, [string]$count)
                if ($count -le [int]$env:JYROXA_TEST_FAILURES) {
                    [Console]::Error.WriteLine($env:JYROXA_TEST_ERROR)
                    exit 1
                }
                [Console]::WriteLine('complete')
            "#,
            ]);
            cmd.env("JYROXA_TEST_ATTEMPTS", &count_path)
                .env("JYROXA_TEST_FAILURES", failures.to_string())
                .env("JYROXA_TEST_ERROR", error);
            let output = output_ytdlp(&mut cmd, url).await.unwrap();
            assert_eq!(output.status.success(), success, "{name}");
            assert_eq!(
                std::fs::read_to_string(count_path).unwrap(),
                expected_attempts.to_string(),
                "{name}"
            );
            if success {
                assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "complete");
            }
        }
    }

    #[test]
    fn dailymotion_probes_use_a_browser_client() {
        for page in [
            "https://www.dailymotion.com/video/x89eyek",
            "https://dailymotion.com/video/x89eyek",
            "https://dai.ly/x89eyek",
        ] {
            assert_eq!(browser_client_args(page), ["--impersonate", "firefox"]);
        }
        for page in [
            "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
            "https://dailymotion.com.example/video/x89eyek",
            "https://example.com/?url=https://dailymotion.com/video/x89eyek",
            "not a URL",
        ] {
            assert!(browser_client_args(page).is_empty(), "{page}");
        }
    }

    #[test]
    fn formats_menu_includes_progressive_and_merged() {
        let info = YtInfo {
            title: Some("Title".into()),
            formats: Some(vec![
                YtFormat {
                    format_id: "22".into(),
                    ext: Some("mp4".into()),
                    height: Some(720),
                    vcodec: Some("avc1".into()),
                    acodec: Some("mp4a".into()),
                    filesize: Some(5_000_000),
                    tbr: Some(800.0),
                    url: Some("https://cdn.example/22.mp4".into()),
                },
                YtFormat {
                    format_id: "140".into(),
                    ext: Some("m4a".into()),
                    height: None,
                    vcodec: Some("none".into()),
                    acodec: Some("mp4a".into()),
                    filesize: Some(1_000_000),
                    tbr: Some(128.0),
                    url: Some("https://cdn.example/140.m4a".into()),
                },
                YtFormat {
                    format_id: "137".into(),
                    ext: Some("mp4".into()),
                    height: Some(1080),
                    vcodec: Some("avc1".into()),
                    acodec: Some("none".into()),
                    filesize: Some(8_000_000),
                    tbr: Some(2500.0),
                    url: None,
                },
            ]),
        };
        let menu = formats_for_menu(&info);
        assert!(menu.iter().any(|c| c.label.contains("720p")));
        assert!(menu.iter().any(|c| c.format_id == "137+140"));
    }
}
