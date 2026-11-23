//! BitTorrent on desktop: CLI sidecar (keeps librqbit out of the Tauri cdylib).

use jyroxa_core::torrent::{
    is_torrent_source, magnet_display_name, parse_error_line, parse_progress_line,
    TorrentProbeInfo, TorrentProgressPhase,
};
use jyroxa_core::Error;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::engine::{AppSettings, DownloadView, TaskStatus};
use crate::sidecar_paths;

pub fn primary_is_torrent(urls: &[String]) -> bool {
    urls.first().map(|u| is_torrent_source(u)).unwrap_or(false)
}

pub fn torrent_filename_hint(urls: &[String]) -> String {
    if let Some(u) = urls.first() {
        if let Some(name) = magnet_display_name(u) {
            if !name.is_empty() {
                return name;
            }
        }
        if u.starts_with("http") && u.to_ascii_lowercase().ends_with(".torrent") {
            return u
                .rsplit('/')
                .next()
                .unwrap_or("download.torrent")
                .to_string();
        }
        if std::path::Path::new(u).extension().is_some() {
            return std::path::Path::new(u)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("torrent")
                .to_string();
        }
    }
    "torrent-download".into()
}

fn jyroxa_cli_exe() -> PathBuf {
    sidecar_paths::jyroxa_cli_exe()
}

/// Spawn `jyroxa torrent get` and stream stderr progress lines back to the UI.
pub async fn probe_torrent_via_sidecar(input: &str) -> Result<TorrentProbeInfo, Error> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::other("empty torrent source"));
    }
    let cli = jyroxa_cli_exe();
    if !cli.is_file() {
        return Err(Error::other(format!(
            "jyroxa CLI not found at {} (set JYROXA_CLI or reinstall; torrent sidecar should sit next to jyroxa-desktop.exe)",
            cli.display()
        )));
    }
    let mut cmd = Command::new(&cli);
    cmd.args(["torrent", "meta", input])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = crate::process_spawn::output_tokio(&mut cmd)
        .await
        .map_err(|e| Error::other(e.to_string()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let reason = if stderr.is_empty() {
            format!("torrent meta probe failed ({})", out.status)
        } else {
            stderr
        };
        return Err(Error::other(reason));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    serde_json::from_str(&text).map_err(|e| Error::other(format!("torrent meta json: {e}")))
}

fn torrent_cli_args(
    input: &str,
    output_dir: &Path,
    only_files: Option<&[usize]>,
    settings: &AppSettings,
) -> Vec<String> {
    let mut args = vec![
        "torrent".into(),
        "get".into(),
        input.to_string(),
        "-o".into(),
        output_dir.to_string_lossy().into_owned(),
    ];
    if let Some(indices) = only_files {
        for idx in indices {
            args.push("-f".into());
            args.push(idx.to_string());
        }
    }
    args.push("--peer-limit".into());
    args.push(settings.torrent_peer_limit.to_string());
    if !settings.torrent_merge_trackers {
        args.push("--no-merge-trackers".into());
    }
    if settings.torrent_seeding {
        args.push("--seed".into());
    }
    args.push("--ratio-limit".into());
    args.push(settings.torrent_ratio_limit.to_string());
    args.push("--session-dir".into());
    args.push(
        torrent_session_dir(output_dir, input)
            .to_string_lossy()
            .into_owned(),
    );
    args
}

pub async fn spawn_torrent_sidecar(
    urls: &[String],
    output_dir: PathBuf,
    only_files: Option<Vec<usize>>,
    settings: &AppSettings,
    cancel: watch::Receiver<bool>,
    on_progress: impl Fn(TorrentProgressLine) + Send + Sync + 'static,
) -> Result<(), Error> {
    let input = urls.first().map(String::as_str).unwrap_or("").trim();
    if input.is_empty() {
        return Err(Error::other("empty torrent source"));
    }
    let cli = jyroxa_cli_exe();
    if !cli.is_file() {
        return Err(Error::other(format!(
            "jyroxa CLI not found at {} (set JYROXA_CLI or reinstall; torrent sidecar should sit next to jyroxa-desktop.exe)",
            cli.display()
        )));
    }
    std::fs::create_dir_all(&output_dir).map_err(|e| Error::other(e.to_string()))?;

    let args = torrent_cli_args(input, &output_dir, only_files.as_deref(), settings);
    let mut cmd = Command::new(&cli);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::process_spawn::configure_tokio(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| Error::other(e.to_string()))?;
    crate::process_spawn::bind_lifetime_to_app(&child);

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(_)) = reader.next_line().await {}
        });
    }

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("torrent sidecar: no stderr"))?;

    let mut reader = BufReader::new(stderr).lines();
    let mut kill = cancel.clone();
    let mut last_error_line: Option<String> = None;

    loop {
        if *kill.borrow() {
            let _ = child.kill().await;
            return Err(Error::Cancelled);
        }
        tokio::select! {
            _ = kill.changed() => {
                if *kill.borrow() {
                    let _ = child.kill().await;
                    return Err(Error::Cancelled);
                }
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(p) = parse_progress_line(&l) {
                            on_progress(TorrentProgressLine::from_core(p));
                        } else if let Some(err) = parse_error_line(&l) {
                            last_error_line = Some(err);
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(Error::other(e.to_string())),
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| Error::other(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        let _ = status;
        Err(Error::other(sidecar_failure_message(last_error_line)))
    }
}

fn sidecar_failure_message(last_error_line: Option<String>) -> String {
    if let Some(msg) = last_error_line.filter(|s| !s.trim().is_empty()) {
        return msg;
    }
    // Console closed / killed / crash — no structured stderr from the sidecar.
    "The torrent engine stopped unexpectedly. Your download was interrupted. Click Retry to resume."
        .into()
}

#[derive(Clone, Debug)]
pub struct TorrentProgressLine {
    pub phase: TorrentProgressPhase,
    pub downloaded: u64,
    pub total: u64,
    pub peers: u32,
    pub seeds: u32,
    pub speed_bps: f64,
}

impl TorrentProgressLine {
    fn from_core(p: jyroxa_core::torrent::TorrentProgressLine) -> Self {
        Self {
            phase: p.phase,
            downloaded: p.downloaded,
            total: p.total,
            peers: p.peers,
            seeds: p.seeds,
            speed_bps: p.speed_bps,
        }
    }
}

pub fn apply_torrent_progress(view: &mut DownloadView, p: &TorrentProgressLine) {
    view.downloaded = p.downloaded;
    if p.total > 0 {
        view.total = Some(p.total);
    }
    view.speed_bps = p.speed_bps;
    view.torrent_peers = Some(p.peers);
    view.torrent_seeds = Some(p.seeds);
    if p.downloaded > 0 {
        view.torrent_ratio = Some(0.0);
    }
    view.status_note = match p.phase {
        TorrentProgressPhase::Metadata => Some(format!(
            "Fetching metadata… ({} peers, {} seeds)",
            p.peers, p.seeds
        )),
        TorrentProgressPhase::Download if p.speed_bps <= 0.0 && p.downloaded == 0 => Some(format!(
            "Connecting to peers… ({} peers, {} seeds)",
            p.peers, p.seeds
        )),
        TorrentProgressPhase::Download if p.speed_bps > 512.0 => None,
        TorrentProgressPhase::Download => view.status_note.clone(),
    };
    if view.status != TaskStatus::Paused {
        view.status = if matches!(p.phase, TorrentProgressPhase::Metadata) {
            TaskStatus::Starting
        } else {
            TaskStatus::Running
        };
    }
}

pub fn mark_torrent_completed(view: &mut DownloadView) {
    view.status = TaskStatus::Completed;
    view.status_note = None;
    if view.total.is_none() && view.downloaded > 0 {
        view.total = Some(view.downloaded);
    }
}

fn torrent_session_dir(output_dir: &Path, source: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    // Source identity survives task renumbering on restart. Separate sidecars
    // sharing a download folder must not read and rewrite one session database.
    let key = format!("{:x}", Sha256::digest(source.as_bytes()));
    output_dir.join(".jyroxa-torrent-session").join(key)
}

/// Sum on-disk bytes in a torrent job folder (for queue restore).
pub fn torrent_bytes_on_disk(dir: &Path) -> u64 {
    if !dir.is_dir() {
        return 0;
    }
    let mut sum = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(read) = std::fs::read_dir(d) else {
            continue;
        };
        for ent in read.flatten() {
            let path = ent.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n == ".jyroxa-torrent-session")
                {
                    continue;
                }
                stack.push(path);
            } else if let Ok(meta) = ent.metadata() {
                sum += meta.len();
            }
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use jyroxa_core::torrent::TorrentProgressLine as CoreLine;

    #[test]
    fn torrent_args_keep_destination_and_isolate_persistent_sessions() {
        let output = Path::new("downloads");
        let source = "magnet:?xt=urn:btih:0000000000000000000000000000000000000001";
        let args = torrent_cli_args(source, output, None, &AppSettings::default());
        assert_eq!(args[4], output.to_string_lossy());
        let session = torrent_session_dir(output, source);
        assert_eq!(args.last().unwrap(), &session.to_string_lossy());
        assert_eq!(session, torrent_session_dir(output, source));
        assert_ne!(session, torrent_session_dir(output, "another source"));
        assert!(session.starts_with(output.join(".jyroxa-torrent-session")));
    }

    #[test]
    fn parses_stderr_progress() {
        let raw = CoreLine {
            phase: TorrentProgressPhase::Metadata,
            state: "initializing".into(),
            downloaded: 0,
            total: 0,
            peers: 4,
            seeds: 1,
            speed_bps: 0.0,
        };
        let parsed = parse_progress_line(&raw.format()).unwrap();
        assert_eq!(parsed.peers, 4);
    }

    #[test]
    fn unexpected_sidecar_exit_is_plain_english_with_retry() {
        let msg = sidecar_failure_message(None);
        assert!(msg.contains("Retry"), "{msg}");
        assert!(msg.contains("unexpectedly"), "{msg}");
        assert_eq!(
            sidecar_failure_message(Some("peer timeout".into())),
            "peer timeout"
        );
    }
}
