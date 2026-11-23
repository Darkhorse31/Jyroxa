#![cfg(feature = "torrent-engine")]

use std::time::Duration;

use jyroxa_core::torrent::{probe_torrent_source_with, TorrentProbeOptions};

#[tokio::test]
async fn local_metadata_does_not_wait_for_dht_bootstrap() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fixture.torrent");
    let mut torrent =
        b"d4:infod6:lengthi1024e4:name11:fixture.bin12:piece lengthi16384e6:pieces20:".to_vec();
    torrent.extend_from_slice(&[0; 20]);
    torrent.extend_from_slice(b"ee");
    std::fs::write(&source, torrent).unwrap();
    let info = tokio::time::timeout(
        Duration::from_secs(4),
        probe_torrent_source_with(
            source.to_str().unwrap(),
            TorrentProbeOptions {
                dht_only: true,
                timeout: Some(Duration::from_secs(2)),
                session_data_dir: Some(dir.path().join("session")),
            },
        ),
    )
    .await
    .expect("local metadata must not wait for public DHT")
    .expect("valid local torrent");
    assert_eq!(info.name, "fixture.bin");
    assert_eq!(info.total_bytes, 1024);
    assert_eq!(info.files.len(), 1);
}

#[tokio::test]
async fn metadata_timeout_stops_session_and_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        probe_torrent_source_with(
            "magnet:?xt=urn:btih:0000000000000000000000000000000000000000",
            TorrentProbeOptions {
                dht_only: true,
                timeout: Some(Duration::from_millis(100)),
                session_data_dir: Some(dir.path().join("session")),
            },
        ),
    )
    .await
    .expect("timed-out metadata must tear down promptly")
    .expect_err("nonexistent swarm must time out");
    assert!(result.to_string().contains("metadata timeout"), "{result}");
}
