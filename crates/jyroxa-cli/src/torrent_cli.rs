//! Headless torrent download and benchmark helpers.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use jyroxa_core::torrent::{
    run_until_done_with_stall, wait_initialized_with_progress, TorrentJobOptions, TorrentSession,
    TorrentSource, TorrentTuning,
};

pub struct TorrentGetOptions {
    pub peer_limit: Option<usize>,
    pub merge_trackers: bool,
    pub allow_seeding: bool,
    pub ratio_limit: f64,
}

impl Default for TorrentGetOptions {
    fn default() -> Self {
        Self {
            peer_limit: None,
            merge_trackers: true,
            allow_seeding: false,
            ratio_limit: 1.0,
        }
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
    let handle = session
        .add(
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
        )
        .await?;

    let meta_start = Instant::now();
    let (_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let emit = |p: jyroxa_core::torrent::TorrentProgressLine| {
        eprintln!("{}", p.format());
    };
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
