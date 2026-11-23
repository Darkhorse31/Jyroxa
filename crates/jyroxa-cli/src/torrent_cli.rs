//! Headless torrent download and benchmark helpers.

use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jyroxa_core::torrent::{
    run_until_done_with_stall, wait_initialized_with_progress, TorrentJobOptions,
    TorrentProgressLine, TorrentProgressPhase, TorrentSession, TorrentSource, TorrentTuning,
    METADATA_TIMEOUT_SECS,
};

pub struct TorrentGetOptions {
    pub peer_limit: Option<usize>,
    pub merge_trackers: bool,
    pub allow_seeding: bool,
    pub ratio_limit: f64,
    pub session_data_dir: Option<PathBuf>,
}

impl Default for TorrentGetOptions {
    fn default() -> Self {
        Self {
            peer_limit: None,
            merge_trackers: true,
            allow_seeding: false,
            ratio_limit: 1.0,
            session_data_dir: None,
        }
    }
}

async fn resolve_with_progress<T>(
    future: impl Future<Output = jyroxa_core::Result<T>>,
    timeout: Duration,
    mut emit: impl FnMut(TorrentProgressLine),
) -> Result<T> {
    tokio::pin!(future);
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let resolution = tokio::time::timeout(timeout, async {
        loop {
            tokio::select! {
                result = &mut future => return result,
                _ = interval.tick() => emit(TorrentProgressLine {
                    phase: TorrentProgressPhase::Metadata,
                    state: "resolving".into(),
                    downloaded: 0,
                    total: 0,
                    peers: 0,
                    seeds: 0,
                    speed_bps: 0.0,
                }),
            }
        }
    })
    .await;
    match resolution {
        Ok(result) => result.map_err(Into::into),
        Err(_) => Err(anyhow::anyhow!(
            "torrent metadata timeout after {}s; check peers and firewall settings",
            timeout.as_secs_f64()
        )),
    }
}

pub async fn run_torrent_meta(input: String) -> Result<()> {
    let info = jyroxa_core::torrent::probe_torrent_source(&input)
        .await
        .context("torrent metadata")?;
    println!("{}", serde_json::to_string(&info)?);
    Ok(())
}

pub async fn run_torrent_get(
    input: String,
    output: PathBuf,
    only_files: Vec<usize>,
    opts: TorrentGetOptions,
) -> Result<()> {
    std::fs::create_dir_all(&output)?;
    let mut tuning = TorrentTuning::default().with_output_root(output.clone());
    if let Some(limit) = opts.peer_limit {
        tuning.peer_limit = limit;
    }
    tuning.merge_builtin_trackers = opts.merge_trackers;
    tuning.allow_seeding = opts.allow_seeding;
    tuning.ratio_limit = opts.ratio_limit;
    if let Some(dir) = opts.session_data_dir {
        tuning.session_data_dir = Some(dir);
    }

    let session = TorrentSession::open(output.clone(), tuning)
        .await
        .context("torrent session")?;
    let source = if input.trim().starts_with("magnet:")
        || input.starts_with("http://")
        || input.starts_with("https://")
    {
        TorrentSource::Url(input.trim())
    } else {
        let bytes = std::fs::read(&input).with_context(|| format!("read {input}"))?;
        TorrentSource::FileBytes(bytes.into())
    };
    let meta_start = Instant::now();
    let emit = |p: TorrentProgressLine| {
        eprintln!("{}", p.format());
    };
    // Magnet resolution happens inside add(), before a handle is returned.
    // Include that work in the metadata deadline and keep stdout/stderr clients
    // informed while DHT and trackers discover peers.
    let add = session.add(
        source,
        TorrentJobOptions {
            output_folder: Some(output.clone()),
            only_files: if only_files.is_empty() {
                None
            } else {
                Some(only_files)
            },
            ..Default::default()
        },
    );
    let resolution =
        resolve_with_progress(add, Duration::from_secs(METADATA_TIMEOUT_SECS), emit).await;
    let handle = match resolution {
        Ok(handle) => handle,
        Err(error) => {
            session.stop().await;
            return Err(error);
        }
    };
    let (_tx, cancel_rx) = tokio::sync::watch::channel(false);
    wait_initialized_with_progress(&handle, cancel_rx, emit).await?;
    let metadata_ms = meta_start.elapsed().as_millis();
    eprintln!("metadata: {metadata_ms} ms");

    let dl_start = Instant::now();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    run_until_done_with_stall(&session, handle.clone(), rx, emit)
        .await
        .context("torrent download")?;
    let elapsed = dl_start.elapsed();
    let snap = jyroxa_core::torrent::TorrentSession::stats(&handle);
    eprintln!(
        "done in {:.1}s avg {:.2} MiB/s peers={} seeds={}",
        elapsed.as_secs_f64(),
        if elapsed.as_secs_f64() > 0.0 {
            snap.downloaded as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0)
        } else {
            0.0
        },
        snap.live_peers,
        snap.live_seeders
    );
    Ok(())
}

pub async fn run_torrent_bench(
    input: String,
    output: PathBuf,
    runs: usize,
    cap_secs: u64,
) -> Result<()> {
    for run in 1..=runs {
        let dir = output.join(format!("run-{run}"));
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("=== jyroxa torrent bench run {run}/{runs} ===");
        let fut = run_torrent_get(
            input.clone(),
            dir.clone(),
            Vec::new(),
            TorrentGetOptions::default(),
        );
        match tokio::time::timeout(std::time::Duration::from_secs(cap_secs), fut).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("run failed: {e:#}"),
            Err(_) => eprintln!("run timed out at {cap_secs}s"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_resolution_emits_metadata_and_has_a_deadline() {
        let mut updates = Vec::new();
        let result = resolve_with_progress(
            std::future::pending::<jyroxa_core::Result<()>>(),
            Duration::from_millis(100),
            |update| updates.push(update),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("metadata timeout"));
        assert!(!updates.is_empty());
        assert!(updates
            .iter()
            .all(|update| update.phase == TorrentProgressPhase::Metadata));
    }

    #[tokio::test]
    async fn resolution_preserves_success_and_errors() {
        assert_eq!(
            resolve_with_progress(async { Ok(42) }, Duration::from_secs(1), |_| {})
                .await
                .unwrap(),
            42
        );
        let result = resolve_with_progress(
            async { Err::<(), _>(jyroxa_core::Error::other("peer failed")) },
            Duration::from_secs(1),
            |_| {},
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("peer failed"));
    }
}
