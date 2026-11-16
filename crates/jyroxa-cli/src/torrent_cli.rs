//! Headless torrent download and benchmark helpers.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use jyroxa_core::torrent::{TorrentJobOptions, TorrentSession, TorrentSource, TorrentTuning};

pub async fn run_torrent_get(input: String, output: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&output)?;
    let tuning = TorrentTuning::default().with_output_root(output.clone());
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
                ..Default::default()
            },
        )
        .await?;
    let meta_start = Instant::now();
    handle.inner().wait_until_initialized().await?;
    let metadata_ms = meta_start.elapsed().as_millis();
    eprintln!("metadata: {metadata_ms} ms");
    let dl_start = Instant::now();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    session
        .run_until_done(handle.clone(), rx, |snap| {
            eprintln!(
                "progress {:.1}% peers={} seeds={} {:.2} MiB/s",
                if snap.total > 0 {
                    snap.downloaded as f64 / snap.total as f64 * 100.0
                } else {
                    0.0
                },
                snap.live_peers,
                snap.live_seeders,
                snap.speed_bps / (1024.0 * 1024.0)
            );
        })
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

pub async fn run_torrent_bench(input: String, output: PathBuf, runs: usize, cap_secs: u64) -> Result<()> {
    for run in 1..=runs {
        let dir = output.join(format!("run-{run}"));
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("=== jyroxa torrent bench run {run}/{runs} ===");
        let fut = run_torrent_get(input.clone(), dir.clone());
        match tokio::time::timeout(std::time::Duration::from_secs(cap_secs), fut).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("run failed: {e:#}"),
            Err(_) => eprintln!("run timed out at {cap_secs}s"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    Ok(())
}