//! yt-dlp sidecar for page-URL video resolution.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use jyroxa_core::events::{EventSender, ProgressEvent};
pub use jyroxa_core::probe::VideoFormatChoice;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Drain both pipes while the hidden child runs. Progress must reach the UI
/// before process exit, and diagnostics must not grow with download duration.
async fn stream_download(
    cmd: &mut tokio::process::Command,
    tx: &EventSender,
) -> std::io::Result<std::process::Output> {
    crate::process_spawn::configure_tokio(cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    crate::process_spawn::bind_lifetime_to_app(&child);
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let mut stderr = BufReader::new(child.stderr.take().expect("piped stderr")).lines();
    let mut tracker = crate::video_progress::Tracker::default();
    let mut diagnostics = String::new();
    let mut stdout_open = true;
    let mut stderr_open = true;
    while stdout_open || stderr_open {
        let line = tokio::select! {
            line = stdout.next_line(), if stdout_open => {
                let line = line?;
                stdout_open = line.is_some();
                line
            },
            line = stderr.next_line(), if stderr_open => {
                let line = line?;
                stderr_open = line.is_some();
                line
            },
        };
        if let Some(line) = line {
            if let Some(progress) = tracker.line(&line) {
                let _ = tx.send(ProgressEvent::VideoProgress { progress });
            } else if !line.starts_with("JX_") {
                diagnostics.push_str(&line);
                diagnostics.push('\n');
                if diagnostics.len() > 65_536 {
                    let mut start = diagnostics.len() - 65_536;
                    while !diagnostics.is_char_boundary(start) {
                        start += 1;
                    }
                    diagnostics.drain(..start);
                }
            }
        }
    }
    Ok(std::process::Output {
        status: child.wait().await?,
        stdout: Vec::new(),
        stderr: diagnostics.into_bytes(),
    })
}

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
            .filter(|f| !vcodec_none(f) && acodec_none(f) && f.height == Some(height))
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
            size_bytes: v
                .filesize
                .zip(a.filesize)
                .and_then(|(v, a)| v.checked_add(a)),
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

/// Shared extraction policy for Add Download, browser probes and actual transfers.
fn configure_extraction(
    cmd: &mut tokio::process::Command,
    page_url: &str,
    headers: &HashMap<String, String>,
) -> Result<Option<tempfile::NamedTempFile>, String> {
    cmd.args(["--ignore-config", "--no-playlist", "--socket-timeout", "20"]);
    cmd.args(browser_client_args(page_url));
    if jyroxa_core::video::is_youtube_page(page_url) {
        if let Some((runtime, path)) = crate::sidecar_paths::javascript_runtime() {
            cmd.arg("--js-runtimes")
                .arg(format!("{runtime}:{}", path.display()));
        }
    }
    // Cookies are opt-in. Never pass secrets on the command line or read a browser
    // profile unless the user selected it in Add Download.
    if let Some(browser) = headers
        .get("X-Jyroxa-Ytdlp-Browser")
        .filter(|b| !b.is_empty())
    {
        if !["chrome", "edge", "brave", "firefox", "chromium"].contains(&browser.as_str()) {
            return Err("Choose a supported browser for YouTube sign-in cookies.".into());
        }
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    let cookie_file = cookie_file(page_url, headers)?;
    if let Some(file) = &cookie_file {
        cmd.arg("--cookies").arg(file.path());
    }
    for (header, flag) in [("User-Agent", "--user-agent"), ("Referer", "--referer")] {
        if let Some(value) = headers.get(header) {
            cmd.arg(flag).arg(value);
        }
    }
    Ok(cookie_file)
}

/// Temporary cookie jars keep extension credentials out of process arguments.
fn cookie_file(
    page_url: &str,
    headers: &HashMap<String, String>,
) -> Result<Option<tempfile::NamedTempFile>, String> {
    use std::io::Write;
    let Some(cookies) = headers.get("Cookie").filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let url = url::Url::parse(page_url).map_err(|e| e.to_string())?;
    let host = url.host_str().ok_or("Video URL has no host")?;
    let (host, subdomains) = if jyroxa_core::video::is_youtube_page(page_url) {
        (".youtube.com", "TRUE")
    } else {
        (host, "FALSE")
    };
    let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    writeln!(file, "# Netscape HTTP Cookie File").map_err(|e| e.to_string())?;
    for part in cookies.split(';') {
        if let Some((name, value)) = part.trim().split_once('=') {
            if name.is_empty()
                || name.contains(['\r', '\n', '\t'])
                || value.contains(['\r', '\n', '\t'])
            {
                return Err("Invalid browser cookies. Send the video from the extension again, then click Retry.".into());
            }
            writeln!(file, "{host}\t{subdomains}\t/\tTRUE\t0\t{name}\t{value}")
                .map_err(|e| e.to_string())?;
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(Some(file))
}

fn extraction_error(stderr: &[u8]) -> String {
    let raw = String::from_utf8_lossy(stderr);
    let lower = raw.to_lowercase();
    let recovery = if lower.contains("not a bot")
        || lower.contains("sign in")
        || lower.contains("login required")
    {
        "YouTube requires sign-in on this network. Choose your signed-in browser under Sign-in cookies, then click Retry."
    } else if lower.contains("decrypt")
        || lower.contains("cookie database")
        || lower.contains("could not copy")
    {
        "Jyroxa could not read browser cookies. Close that browser and click Retry, or choose another signed-in browser. Chrome may protect cookies from other apps."
    } else if lower.contains("addondisposecallback is not a function") {
        "YouTube changed its player challenge and Jyroxa's yt-dlp sidecar is outdated. Update Jyroxa to the latest version, then click Retry."
    } else if lower.contains("javascript") || lower.contains("challenge") || lower.contains("ejs") {
        "YouTube's player challenge could not be solved. Use a current yt-dlp with its EJS scripts and a supported Node or Deno runtime beside Jyroxa, then click Retry."
    } else if lower.contains("403") || lower.contains("429") {
        "YouTube refused this request. Wait a moment, choose your signed-in browser if needed, then click Retry."
    } else {
        "Video fetch failed. Check the URL and connection, then click Retry."
    };
    format!("{recovery}\n{}", raw.trim())
}

pub async fn probe_page_formats(page_url: &str) -> Result<YtInfo, String> {
    probe_page_formats_with_headers(page_url, &HashMap::new()).await
}

async fn probe_page_formats_with_headers(
    page_url: &str,
    headers: &HashMap<String, String>,
) -> Result<YtInfo, String> {
    let canonical = jyroxa_core::video::youtube_page_url(page_url);
    let page_url = canonical.as_deref().unwrap_or(page_url);
    let ytdlp = yt_dlp_path();
    if !ytdlp.is_file() {
        return Err(
            "yt-dlp.exe is missing. Repair Jyroxa's video sidecars, then click Retry.".into(),
        );
    }
    let mut cmd = tokio::process::Command::new(&ytdlp);
    let _cookies = configure_extraction(&mut cmd, page_url, headers)?;
    cmd.args(["-J", "--", page_url]);
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        output_ytdlp(&mut cmd, page_url),
    )
    .await
    .map_err(|_| "Video fetch timed out. Check your connection, then click Retry.".to_string())?
    .map_err(|e| format!("Could not start yt-dlp: {e}. Repair Jyroxa, then click Retry."))?;
    if !out.status.success() {
        return Err(extraction_error(&out.stderr));
    }
    let info: YtInfo = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("Invalid video metadata: {e}. Click Retry."))?;
    if info
        .formats
        .as_ref()
        .map_or(true, |f| f.iter().all(|f| vcodec_none(f) && acodec_none(f)))
    {
        return Err(extraction_error(
            b"No playable video formats were returned by yt-dlp.",
        ));
    }
    Ok(info)
}

pub async fn probe_video(
    page_url: &str,
    headers: &HashMap<String, String>,
) -> Result<jyroxa_core::ProbeInfo, String> {
    let info = probe_page_formats_with_headers(page_url, headers).await?;
    Ok(jyroxa_core::ProbeInfo {
        final_url: page_url.into(),
        suggested_name: sanitize_filename(&format!(
            "{}.mp4",
            info.title.as_deref().unwrap_or("video")
        )),
        size: None,
        mime: Some("video/mp4".into()),
        resumable: true,
        category: jyroxa_core::Category::Video,
        server: "yt-dlp".into(),
        protocol: "HTTPS".into(),
        telegram_login_required: false,
        video_formats: Some(formats_for_menu(&info)),
    })
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
    jyroxa_core::naming::sanitize_filename(name)
}

pub async fn download_via_ytdlp_with_headers(
    page_url: &str,
    output: &Path,
    format_id: Option<&str>,
    headers: &HashMap<String, String>,
    tx: EventSender,
) -> Result<u64, String> {
    let canonical = jyroxa_core::video::youtube_page_url(page_url);
    let page_url = canonical.as_deref().unwrap_or(page_url);
    let ytdlp = yt_dlp_path();
    if !ytdlp.is_file() {
        return Err("yt-dlp.exe not bundled".into());
    }
    let ffmpeg = ffmpeg_path();
    if !ffmpeg.is_file() {
        return Err(
            "ffmpeg.exe is missing. Repair Jyroxa's video sidecars, then click Retry.".into(),
        );
    }
    let mut args = vec![
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
        if output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        {
            args.push("--remux-video".into());
            args.push("mp4".into());
        }
    }
    let out_arg = if output.extension().is_some() {
        output.to_string_lossy().replace('%', "%%")
    } else {
        return Err("Video output needs a file extension, such as .mp4.".into());
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
    args.push("--".into());
    args.push(page_url.to_string());
    let mut cmd = tokio::process::Command::new(&ytdlp);
    let _cookies = configure_extraction(&mut cmd, page_url, headers)?;
    cmd.args([
        "--newline",
        "--progress",
        "--progress-delta",
        "0.25",
        "--no-simulate",
        "--output-na-placeholder",
        "null",
        "--print",
        crate::video_progress::PLAN_TEMPLATE,
        "--progress-template",
        crate::video_progress::DOWNLOAD_TEMPLATE,
        "--progress-template",
        crate::video_progress::POST_TEMPLATE,
    ]);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut out = None;
    for attempt in 0..3 {
        let _ = tx.send(ProgressEvent::VideoProgress {
            progress: crate::video_progress::Tracker::fetching(),
        });
        let result = stream_download(&mut cmd, &tx)
            .await
            .map_err(|e| e.to_string())?;
        let error = String::from_utf8_lossy(&result.stderr);
        let refresh = !browser_client_args(page_url).is_empty()
            && ["HTTP Error 403", "HTTP Error 429", "No video formats found"]
                .iter()
                .any(|message| error.contains(message));
        let done = result.status.success() || !refresh || attempt == 2;
        out = Some(result);
        if done {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
    }
    let out = out.expect("at least one download attempt");
    if !out.status.success() {
        return Err(extraction_error(&out.stderr));
    }
    if let Ok(m) = tokio::fs::metadata(output).await {
        if m.is_file() && m.len() > 0 {
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
    async fn download_progress_arrives_before_exit_and_drains_diagnostics() {
        let (tx, mut rx) = jyroxa_core::events::channel();
        let task = tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", r#"
                [Console]::Out.WriteLine('JX_PROGRESS:{"id":"18","downloaded":512,"total":1024,"speed":256}')
                [Console]::Out.Flush()
                Start-Sleep -Seconds 3
                for ($i=0; $i -lt 3000; $i++) { [Console]::Error.WriteLine('diagnostic fixture line draining both pipes') }
                [Console]::Out.WriteLine('JX_POST:Merger')
                [Console]::Error.WriteLine('fixture terminal failure')
                exit 7
            "#]);
            stream_download(&mut cmd, &tx).await.unwrap()
        });
        let first = tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let ProgressEvent::VideoProgress { progress } = first else {
            panic!("video progress required")
        };
        assert_eq!((progress.downloaded, progress.total), (512, Some(1024)));
        assert!(
            !task.is_finished(),
            "progress must arrive while the child is running"
        );
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert!(output.stderr.len() <= 65_536);
        assert!(String::from_utf8_lossy(&output.stderr).contains("fixture terminal failure"));
        let ProgressEvent::VideoProgress { progress } = rx.recv().await.unwrap() else {
            panic!("processing progress required")
        };
        assert_eq!(progress.phase, "merging");
    }

    #[test]
    fn browser_cookies_use_a_temporary_jar_not_process_arguments() {
        let headers = HashMap::from([("Cookie".into(), "SID=test-value; theme=dark".into())]);
        let mut cmd = tokio::process::Command::new("yt-dlp.exe");
        let file = configure_extraction(&mut cmd, "https://www.youtube.com/watch?v=abc", &headers)
            .unwrap()
            .unwrap();
        let path = file.path().to_owned();
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--cookies".to_string()));
        assert!(!args.iter().any(|a| a.contains("test-value")));
        let jar = std::fs::read_to_string(&path).unwrap();
        assert!(jar.contains(".youtube.com\tTRUE\t/\tTRUE\t0\tSID\ttest-value"));
        drop(file);
        assert!(!path.exists());
        let malicious = HashMap::from([("Cookie".into(), "SID=value\nforged".into())]);
        assert!(cookie_file("https://www.youtube.com/watch?v=abc", &malicious).is_err());
    }

    #[test]
    fn extraction_policy_and_recovery_are_shared() {
        let mut cmd = tokio::process::Command::new("yt-dlp.exe");
        configure_extraction(&mut cmd, "https://youtu.be/abc", &HashMap::new()).unwrap();
        let args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("--ignore-config")));
        assert!(!args.contains(&std::ffi::OsStr::new("--cookies-from-browser")));
        assert!(!args.contains(&std::ffi::OsStr::new("--flat-playlist")));
        for (error, expected) in [
            ("Sign in to confirm you're not a bot", "sign-in"),
            (
                "Could not copy Chrome cookie database",
                "could not read browser cookies",
            ),
            (
                "JS challenge: JavaScript runtime unavailable",
                "player challenge",
            ),
            (
                "TypeError: a.addOnDisposeCallback is not a function",
                "sidecar is outdated",
            ),
            ("HTTP Error 403", "refused"),
            ("Video removed", "Check the URL"),
        ] {
            let message = extraction_error(error.as_bytes());
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("Retry"));
            assert!(message.contains(error));
        }
        let invalid = HashMap::from([(
            "X-Jyroxa-Ytdlp-Browser".into(),
            "chrome --exec malicious".into(),
        )]);
        assert!(configure_extraction(&mut cmd, "https://youtu.be/abc", &invalid).is_err());
    }

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
