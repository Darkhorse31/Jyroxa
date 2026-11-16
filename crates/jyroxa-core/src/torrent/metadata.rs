//! Resolve torrent metadata (name, size, file list) without downloading payload.

use std::time::Duration;

use crate::error::{Error, Result};
use crate::torrent::probe_types::{TorrentFileProbe, TorrentProbeInfo};
use crate::torrent::config::TorrentTuning;
use crate::torrent::engine::{TorrentJobOptions, TorrentSession, TorrentSource};
use crate::torrent::magnet::{is_magnet_uri, magnet_display_name};

/// Fetch torrent metadata from a magnet, `.torrent` URL/path, or raw HTTP(S) torrent URL.
pub async fn probe_torrent_source(input: &str) -> Result<TorrentProbeInfo> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::other("empty torrent source"));
    }

    let probe_dir = std::env::temp_dir().join(format!(
        "jyroxa-torrent-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&probe_dir);
    std::fs::create_dir_all(&probe_dir).map_err(|e| Error::other(e.to_string()))?;

    let tuning = TorrentTuning::default().with_output_root(probe_dir.clone());
    let session = TorrentSession::open(probe_dir.clone(), tuning)
        .await
        .map_err(|e| Error::other(e.to_string()))?;

    let source = if is_magnet_uri(input)
        || input.starts_with("http://")
        || input.starts_with("https://")
    {
        TorrentSource::Url(input)
    } else if std::path::Path::new(input).is_file() {
        let bytes = std::fs::read(input).map_err(|e| Error::other(e.to_string()))?;
        TorrentSource::FileBytes(bytes.into())
    } else {
        let _ = std::fs::remove_dir_all(&probe_dir);
        return Err(Error::other(format!("torrent source not found: {input}")));
    };

    let handle = session
        .add(
            source,
            TorrentJobOptions {
                overwrite: false,
                output_folder: Some(probe_dir.clone()),
                ..Default::default()
            },
        )
        .await?;

    tokio::time::timeout(
        Duration::from_secs(120),
        handle.inner().wait_until_initialized(),
    )
    .await
    .map_err(|_| Error::other("torrent metadata timeout"))?
    .map_err(|e| Error::other(e.to_string()))?;

    let info = handle
        .inner()
        .with_metadata(|m| {
            let name = handle
                .inner()
                .name()
                .filter(|n| !n.is_empty())
                .or_else(|| magnet_display_name(input))
                .unwrap_or_else(|| "torrent".into());
            let files = m
                .file_infos
                .iter()
                .enumerate()
                .map(|(index, f)| TorrentFileProbe {
                    index,
                    path: f.relative_filename.to_string_lossy().into_owned(),
                    length: f.len,
                })
                .collect();
            let total_bytes: u64 = m.file_infos.iter().map(|f| f.len).sum();
            TorrentProbeInfo {
                name,
                total_bytes,
                files,
            }
        })
        .map_err(|e| Error::other(e.to_string()))?;

    drop(handle);
    drop(session);
    let _ = std::fs::remove_dir_all(&probe_dir);

    Ok(info)
}