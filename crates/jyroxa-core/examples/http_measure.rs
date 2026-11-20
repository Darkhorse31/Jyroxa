//! Headless HTTP measurement with fresh in-memory host statistics.
use std::path::PathBuf;
use std::time::Instant;

use jyroxa_core::{events, Control, DownloadConfig, DownloadRequest, Downloader, ProgressEvent};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: http_measure URL OUTPUT [CONNECTIONS] [SHA256]".into());
    }
    let connections = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(32);
    let engine = Downloader::new(DownloadConfig::default())?;
    let mut request =
        DownloadRequest::new(&args[1], PathBuf::from(&args[2])).connections(connections);
    request.sha256 = args.get(4).cloned();
    let (tx, mut rx) = events::channel();
    let start = Instant::now();
    let mut first_50mb = None;
    let mut resumed_from = 0;
    let consumer = async {
        while let Some(event) = rx.recv().await {
            match event {
                ProgressEvent::Started {
                    resumed_from: bytes,
                    ..
                } => resumed_from = bytes,
                ProgressEvent::Progress { downloaded, .. }
                    if downloaded >= 50_000_000 && first_50mb.is_none() =>
                {
                    first_50mb = Some(start.elapsed().as_secs_f64());
                }
                _ => {}
            }
        }
    };
    let (result, ()) = tokio::join!(
        engine.download(request, Some(tx), Control::new(), None),
        consumer
    );
    let outcome = result?;
    let seconds = start.elapsed().as_secs_f64();
    let digest =
        jyroxa_core::checksum::hash_file(&outcome.output, jyroxa_core::checksum::Algo::Sha256)?;
    println!(
        "{}",
        serde_json::json!({
            "url": args[1], "bytes": outcome.total_size, "seconds": seconds,
            "first_50mb_seconds": first_50mb.or_else(|| (outcome.total_size >= 50_000_000).then_some(seconds)),
            "mbps": outcome.total_size as f64 / seconds / 1_000_000.0,
            "connections": outcome.connections_used, "sha256": digest,
            "resumed": outcome.resumed, "resumed_from": resumed_from
        })
    );
    Ok(())
}
