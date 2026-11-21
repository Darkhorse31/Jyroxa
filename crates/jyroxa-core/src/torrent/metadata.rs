//! Resolve torrent metadata (name, size, file list) without downloading payload.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::torrent::config::TorrentTuning;
use crate::torrent::engine::{TorrentJobOptions, TorrentSession, TorrentSource};
use crate::torrent::magnet::{is_magnet_uri, magnet_display_name};
use crate::torrent::probe_types::{TorrentFileProbe, TorrentProbeInfo};
use crate::torrent::sidecar_proto::{DHT_COLD_METADATA_TIMEOUT_SECS, METADATA_TIMEOUT_SECS};

/// Options for [`probe_torrent_source_with`].
#[derive(Clone, Debug)]
pub struct TorrentProbeOptions {
    /// Disable tracker traffic; resolve metadata via DHT (and LSD/PEX) only.
    pub dht_only: bool,
    /// Metadata wait budget. `None` picks tracker-assisted vs DHT-cold defaults.
    pub timeout: Option<Duration>,
    /// Override the persisted DHT session folder (isolated sidecar routing table).
    pub session_data_dir: Option<PathBuf>,
}

impl Default for TorrentProbeOptions {
    fn default() -> Self {
        Self {
            dht_only: false,
            timeout: None,
            session_data_dir: None,
        }
    }
}

/// Stable, process-isolated DHT routing directory (not under the ephemeral probe folder).
/// One meta probe / sidecar at a time should use this so cold bootstrap can warm and persist.
pub fn isolated_dht_session_dir() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join("Jyroxa").join("dht-session")
}

/// Fetch torrent metadata from a magnet, `.torrent` URL/path, or raw HTTP(S) torrent URL.
pub async fn probe_torrent_source(input: &str) -> Result<TorrentProbeInfo> {
    probe_torrent_source_with(input, TorrentProbeOptions::default()).await
}

/// Fetch torrent metadata with explicit DHT / timeout / session isolation options.
pub async fn probe_torrent_source_with(
    input: &str,
    options: TorrentProbeOptions,
) -> Result<TorrentProbeInfo> {
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

    let session_data_dir = options
        .session_data_dir
        .unwrap_or_else(isolated_dht_session_dir);
    let _ = std::fs::create_dir_all(&session_data_dir);

    let mut tuning = TorrentTuning::default();
    tuning.session_data_dir = Some(session_data_dir.clone());
    tuning.fastresume = false;
    // Meta probes must not wait on UPnP / dual-stack uTP bring-up.
    tuning.enable_upnp = false;
    tuning.tcp_listen_only = true;
    if options.dht_only {
        tuning.merge_builtin_trackers = false;
        tuning.disable_trackers = true;
    }

    let dht_store = session_data_dir.join("dht-routing.json");
    let cold_dht = !dht_store.is_file()
        || std::fs::metadata(&dht_store)
            .map(|m| m.len() < 64)
            .unwrap_or(true);

    let timeout = options.timeout.unwrap_or_else(|| {
        if options.dht_only || cold_dht {
            Duration::from_secs(DHT_COLD_METADATA_TIMEOUT_SECS)
        } else {
            Duration::from_secs(METADATA_TIMEOUT_SECS)
        }
    });

    let result = tokio::time::timeout(timeout, async {
        let session = TorrentSession::open(probe_dir.clone(), tuning)
            .await
            .map_err(|e| Error::other(e.to_string()))?;

        // Cold DHT: let bootstrap fill the routing table before get_peers races an empty table.
        if cold_dht {
            tokio::time::sleep(Duration::from_secs(45)).await;
        } else {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        let source = if is_magnet_uri(input)
            || input.starts_with("http://")
            || input.starts_with("https://")
        {
            TorrentSource::Url(input)
        } else if std::path::Path::new(input).is_file() {
            let bytes = std::fs::read(input).map_err(|e| Error::other(e.to_string()))?;
            TorrentSource::FileBytes(bytes.into())
        } else {
            session.stop().await;
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

        handle
            .inner()
            .wait_until_initialized()
            .await
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
                    info_hash: Some(handle.inner().info_hash().as_string()),
                }
            })
            .map_err(|e| Error::other(e.to_string()))?;

        drop(handle);
        session.stop().await;
        Ok(info)
    })
    .await;

    let _ = std::fs::remove_dir_all(&probe_dir);

    match result {
        Ok(Ok(info)) => Ok(info),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Error::other("torrent metadata timeout")),
    }
}
