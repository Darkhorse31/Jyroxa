//! Desktop integration for jyroxa-core BitTorrent sessions.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use jyroxa_core::torrent::{
    is_torrent_source, magnet_display_name, TorrentJobOptions, TorrentSession, TorrentSource,
    TorrentTuning,
};
use jyroxa_core::Error;

use crate::engine::{AppSettings, DownloadView, TaskStatus};

pub fn primary_is_torrent(urls: &[String]) -> bool {
    urls.first()
        .map(|u| is_torrent_source(u))
        .unwrap_or(false)
}

pub async fn torrent_session(
    slot: &Arc<Mutex<Option<Arc<TorrentSession>>>>,
    output_root: PathBuf,
    settings: &AppSettings,
) -> Result<Arc<TorrentSession>, Error> {
    let mut guard = slot.lock().await;
    if let Some(s) = guard.as_ref() {
        return Ok(Arc::clone(s));
    }
    let mut tuning = TorrentTuning::default().with_output_root(output_root.clone());
    tuning.peer_limit = settings.torrent_peer_limit.max(16);
    tuning.merge_builtin_trackers = settings.torrent_merge_trackers;
    tuning.allow_seeding = settings.torrent_seeding;
    tuning.ratio_limit = settings.torrent_ratio_limit;
    if let Some(limit) = settings.global_speed_limit {
        tuning.download_limit_bps = limit;
    }
    let session = Arc::new(TorrentSession::open(output_root, tuning).await?);
    *guard = Some(Arc::clone(&session));
    Ok(session)
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

pub async fn start_torrent(
    session: &TorrentSession,
    urls: &[String],
    output_dir: PathBuf,
) -> Result<(jyroxa_core::torrent::TorrentHandle, watch::Sender<bool>, watch::Receiver<bool>), Error> {
    let source = if let Some(u) = urls.first() {
        let u = u.trim();
        if u.starts_with("magnet:") || u.starts_with("http://") || u.starts_with("https://") {
            TorrentSource::Url(u)
        } else if std::path::Path::new(u).is_file() {
            let bytes = std::fs::read(u).map_err(|e| Error::other(e.to_string()))?;
            TorrentSource::FileBytes(bytes.into())
        } else {
            TorrentSource::Url(u)
        }
    } else {
        return Err(Error::other("empty torrent source"));
    };
    let opts = TorrentJobOptions {
        output_folder: Some(output_dir),
        overwrite: true,
        ..Default::default()
    };
    let handle = session.add(source, opts).await?;
    let (tx, rx) = watch::channel(false);
    Ok((handle, tx, rx))
}

pub fn apply_torrent_snapshot(view: &mut DownloadView, snap: &jyroxa_core::torrent::TorrentSnapshot) {
    view.downloaded = snap.downloaded;
    view.total = Some(snap.total);
    view.speed_bps = snap.speed_bps;
    view.torrent_peers = Some(snap.live_peers);
    view.torrent_seeds = Some(snap.live_seeders);
    if snap.downloaded > 0 {
        view.torrent_ratio = Some(snap.uploaded as f64 / snap.downloaded as f64);
    }
    if snap.finished {
        view.status = TaskStatus::Completed;
    } else if view.status != TaskStatus::Paused {
        view.status = TaskStatus::Running;
    }
    if let Some(err) = &snap.error {
        view.status = TaskStatus::Failed;
        view.error = Some(err.clone());
    }
}