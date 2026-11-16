//! BitTorrent on desktop: CLI sidecar (keeps librqbit out of the Tauri cdylib).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use jyroxa_core::torrent::{is_torrent_source, magnet_display_name};
use jyroxa_core::Error;
use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::engine::{AppSettings, DownloadView, TaskStatus};

pub fn primary_is_torrent(urls: &[String]) -> bool {
    urls.first()
        .map(|u| is_torrent_source(u))
        .unwrap_or(false)
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
    if let Ok(p) = std::env::var("JYROXA_CLI") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return path;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["jyroxa.exe", "jyroxa-cli.exe"] {
                let c = dir.join(name);
                if c.is_file() {
                    return c;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target/release/jyroxa.exe")
}

/// Spawn `jyroxa torrent get` and stream stderr progress lines back to the UI.
pub async fn spawn_torrent_sidecar(
    urls: &[String],
    output_dir: PathBuf,
    _settings: &AppSettings,
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
            "jyroxa CLI not found at {} (set JYROXA_CLI)",
            cli.display()
        )));
    }
    std::fs::create_dir_all(&output_dir).map_err(|e| Error::other(e.to_string()))?;

    let mut child = Command::new(&cli)
        .arg("torrent")
        .arg("get")
        .arg(input)
        .arg("-o")
        .arg(&output_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::other(e.to_string()))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("torrent sidecar: no stderr"))?;
    let progress_re = Regex::new(
        r"^jyroxa-torrent-progress downloaded=(\d+) total=(\d+) peers=(\d+) seeds=(\d+) speed_bps=(\d+)$",
    )
    .map_err(|e| Error::other(e.to_string()))?;

    let mut reader = BufReader::new(stderr).lines();
    let mut kill = cancel.clone();

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
                        if let Some(caps) = progress_re.captures(&l) {
                            let progress = TorrentProgressLine {
                                downloaded: caps[1].parse().unwrap_or(0),
                                total: caps[2].parse().unwrap_or(0),
                                peers: caps[3].parse().unwrap_or(0),
                                seeds: caps[4].parse().unwrap_or(0),
                                speed_bps: caps[5].parse().unwrap_or(0.0),
                            };
                            on_progress(progress);
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
        Err(Error::other(format!(
            "torrent sidecar exited with {}",
            status
        )))
    }
}

#[derive(Clone, Debug)]
pub struct TorrentProgressLine {
    pub downloaded: u64,
    pub total: u64,
    pub peers: u32,
    pub seeds: u32,
    pub speed_bps: f64,
}

pub fn apply_torrent_progress(view: &mut DownloadView, p: &TorrentProgressLine) {
    view.downloaded = p.downloaded;
    view.total = Some(p.total);
    view.speed_bps = p.speed_bps;
    view.torrent_peers = Some(p.peers);
    view.torrent_seeds = Some(p.seeds);
    if p.downloaded > 0 {
        view.torrent_ratio = Some(0.0);
    }
    if view.status != TaskStatus::Paused {
        view.status = TaskStatus::Running;
    }
}

pub fn mark_torrent_completed(view: &mut DownloadView) {
    view.status = TaskStatus::Completed;
    if view.total.is_none() && view.downloaded > 0 {
        view.total = Some(view.downloaded);
    }
}

pub fn sidecar_output_dir(data_dir: &Path, id: u64) -> PathBuf {
    data_dir.join("torrents").join(id.to_string())
}