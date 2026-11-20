//! Poll loops with timeouts for CLI / desktop sidecar.

use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::torrent::engine::{TorrentHandle, TorrentSession, TorrentSnapshot};
use crate::torrent::sidecar_proto::{
    stall_user_message, TorrentProgressLine, TorrentProgressPhase, METADATA_TIMEOUT_SECS,
    STALL_TIMEOUT_SECS,
};

fn snap_to_progress(phase: TorrentProgressPhase, snap: &TorrentSnapshot) -> TorrentProgressLine {
    TorrentProgressLine {
        phase,
        state: snap.state.clone(),
        downloaded: snap.downloaded,
        total: snap.total,
        peers: snap.live_peers,
        seeds: snap.live_seeders,
        speed_bps: snap.speed_bps,
    }
}

/// Wait for librqbit metadata while emitting peer/state progress (magnet DHT phase).
pub async fn wait_initialized_with_progress(
    handle: &TorrentHandle,
    mut cancel: watch::Receiver<bool>,
    mut on_tick: impl FnMut(TorrentProgressLine) + Send,
) -> Result<()> {
    let started = Instant::now();
    let init = handle.inner().wait_until_initialized();
    tokio::pin!(init);

    loop {
        if *cancel.borrow() {
            return Err(Error::Cancelled);
        }
        if started.elapsed() >= Duration::from_secs(METADATA_TIMEOUT_SECS) {
            let snap = TorrentSession::stats(handle);
            return Err(Error::other(stall_user_message(
                METADATA_TIMEOUT_SECS,
                snap.live_peers,
                TorrentProgressPhase::Metadata,
            )));
        }

        let snap = TorrentSession::stats(handle);
        on_tick(snap_to_progress(TorrentProgressPhase::Metadata, &snap));

        if let Some(err) = &snap.error {
            return Err(Error::other(err.clone()));
        }

        tokio::select! {
            r = &mut init => {
                on_tick(snap_to_progress(
                    TorrentProgressPhase::Download,
                    &TorrentSession::stats(handle),
                ));
                return r.map_err(|e| Error::other(e.to_string()));
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            _ = cancel.changed() => {}
        }
    }
}

/// Download loop with stall detection (never spin at 0 B/s without eventual failure).
pub async fn run_until_done_with_stall(
    session: &TorrentSession,
    handle: TorrentHandle,
    mut cancel: watch::Receiver<bool>,
    mut on_tick: impl FnMut(TorrentProgressLine) + Send + Sync + 'static,
) -> Result<()> {
    let ratio_limit = session.tuning().ratio_limit;
    let allow_seed = session.tuning().allow_seeding;

    let mut last_progress_bytes = 0u64;
    let mut stall_since: Option<Instant> = None;

    loop {
        if *cancel.borrow() {
            return Err(Error::Cancelled);
        }

        let snap = TorrentSession::stats(&handle);
        on_tick(snap_to_progress(TorrentProgressPhase::Download, &snap));

        if snap.finished {
            if !allow_seed {
                return Ok(());
            }
            if ratio_limit > 0.0 && snap.downloaded > 0 {
                let ratio = snap.uploaded as f64 / snap.downloaded as f64;
                if ratio >= ratio_limit {
                    return Ok(());
                }
            } else if ratio_limit <= 0.0 {
                return Ok(());
            }
        }

        if let Some(err) = &snap.error {
            return Err(Error::other(err.clone()));
        }

        let moving = snap.speed_bps > 1024.0 || snap.downloaded > last_progress_bytes;
        if moving {
            last_progress_bytes = snap.downloaded;
            stall_since = None;
        } else if snap.total > 0 && !snap.finished {
            let entry = stall_since.get_or_insert_with(Instant::now);
            if entry.elapsed() >= Duration::from_secs(STALL_TIMEOUT_SECS) {
                return Err(Error::other(stall_user_message(
                    STALL_TIMEOUT_SECS,
                    snap.live_peers,
                    TorrentProgressPhase::Download,
                )));
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            _ = cancel.changed() => {}
        }
    }
}
