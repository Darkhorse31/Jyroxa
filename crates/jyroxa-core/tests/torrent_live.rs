//! Live BitTorrent metadata (network). Current Debian netinst info hash without tracker params.

use std::time::Duration;

use jyroxa_core::torrent::{
    probe_torrent_source_with, DHT_COLD_METADATA_TIMEOUT_SECS, TorrentProbeOptions,
};

/// debian-13.7.0-amd64-netinst.iso (cdimage.debian.org current amd64 bt-cd).
const DEBIAN_DHT_MAGNET: &str = "magnet:?xt=urn:btih:7acf8fb590b2060dd9c3146ef770169d593433b0";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dht_only_debian_magnet_metadata() {
    if std::env::var("JYROXA_TORRENT_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skip dht_only_debian_magnet_metadata (set JYROXA_TORRENT_LIVE=1)");
        return;
    }

    // Reuse the isolated persisted DHT store so a prior warm table speeds cold start.
    let session_data_dir = jyroxa_core::torrent::isolated_dht_session_dir();
    let _ = std::fs::create_dir_all(&session_data_dir);

    let info = probe_torrent_source_with(
        DEBIAN_DHT_MAGNET,
        TorrentProbeOptions {
            dht_only: true,
            timeout: Some(Duration::from_secs(DHT_COLD_METADATA_TIMEOUT_SECS)),
            session_data_dir: Some(session_data_dir),
        },
    )
    .await
    .expect("debian dht magnet metadata");
    assert!(
        info.total_bytes > 300_000_000,
        "unexpected netinst size {}",
        info.total_bytes
    );
    assert!(
        info.name.to_ascii_lowercase().contains("debian") || info.name.ends_with(".iso"),
        "name={}",
        info.name
    );
    assert_eq!(
        info.info_hash.as_deref(),
        Some("7acf8fb590b2060dd9c3146ef770169d593433b0")
    );
}
