//! Live BitTorrent metadata (network), independently checked against Debian metainfo.
#![cfg(feature = "torrent-engine")]

use std::time::Duration;

use jyroxa_core::torrent::{
    probe_torrent_source_with, TorrentProbeOptions, DHT_COLD_METADATA_TIMEOUT_SECS,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dht_only_debian_magnet_metadata() {
    if std::env::var("JYROXA_TORRENT_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skip dht_only_debian_magnet_metadata (set JYROXA_TORRENT_LIVE=1)");
        return;
    }

    // Discover the current official image rather than testing a retired swarm.
    // The downloaded metainfo establishes identity only; the probe receives just
    // its info hash, with tracker discovery disabled.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let base = "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/";
    let listing = client
        .get(base)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();
    let pattern =
        regex::Regex::new(r#"href="(debian-[0-9.]+-amd64-netinst\.iso\.torrent)""#).unwrap();
    let filename = &pattern
        .captures(&listing)
        .expect("official Debian netinst torrent")[1];
    let source = format!("{base}{filename}");
    let bytes = client
        .get(&source)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let expected = librqbit::torrent_from_bytes(&bytes).expect("official torrent metainfo");
    let hash = expected.info_hash.as_string();
    let magnet = format!("magnet:?xt=urn:btih:{hash}");
    eprintln!("DHT-only source={source} info_hash={hash}");

    // A fresh table proves cold bootstrap and avoids touching the user's cache.
    let session_data_dir = tempfile::tempdir().unwrap();

    let info = probe_torrent_source_with(
        &magnet,
        TorrentProbeOptions {
            dht_only: true,
            timeout: Some(Duration::from_secs(DHT_COLD_METADATA_TIMEOUT_SECS)),
            session_data_dir: Some(session_data_dir.path().to_path_buf()),
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
    assert_eq!(info.info_hash.as_deref(), Some(hash.as_str()));
}
