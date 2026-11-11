//! End-to-end tests against a local axum server with Range support.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use swiftget_core::download::{preallocate, Control};
use swiftget_core::{
    DownloadConfig, DownloadRequest, DownloadState, Downloader, Error, SegmentState,
};

#[derive(Clone)]
struct AppState {
    data: Arc<Vec<u8>>,
    etag: String,
}

fn make_data(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn parse_range(s: &str, total: u64) -> Option<(u64, u64)> {
    let r = s.strip_prefix("bytes=")?;
    let (a, b) = r.split_once('-')?;
    let start: u64 = a.trim().parse().ok()?;
    let end = if b.trim().is_empty() {
        total - 1
    } else {
        b.trim().parse().ok()?
    }
    .min(total - 1);
    if start > end {
        None
    } else {
        Some((start, end))
    }
}

fn build_response(data: &[u8], etag: &str, range: Option<&str>, allow_ranges: bool) -> Response {
    let total = data.len() as u64;
    if allow_ranges {
        if let Some(r) = range {
            if let Some((s, e)) = parse_range(r, total) {
                let slice = data[s as usize..=e as usize].to_vec();
                return Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_LENGTH, slice.len().to_string())
                    .header(header::ETAG, etag)
                    .body(Body::from(slice))
                    .unwrap();
            }
        }
    }
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, total.to_string())
        .header(header::ETAG, etag);
    if allow_ranges {
        builder = builder.header(header::ACCEPT_RANGES, "bytes");
    }
    builder.body(Body::from(data.to_vec())).unwrap()
}

async fn serve_file(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    build_response(&st.data, &st.etag, range, true)
}

async fn serve_noranges(State(st): State<AppState>, _headers: HeaderMap) -> Response {
    // Ignores Range entirely and never advertises Accept-Ranges.
    build_response(&st.data, &st.etag, None, false)
}

async fn spawn_server(data: Vec<u8>) -> SocketAddr {
    let st = AppState {
        data: Arc::new(data),
        etag: "\"v1\"".into(),
    };
    let app = Router::new()
        .route("/file", get(serve_file))
        .route("/noranges", get(serve_noranges))
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn test_engine() -> Downloader {
    let config = DownloadConfig {
        min_segment_size: 16 * 1024,
        initial_connections: 4,
        max_connections: 8,
        tune_interval: Duration::from_millis(40),
        ..DownloadConfig::default()
    };
    Downloader::new(config).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_connection_download_is_correct() {
    let data = make_data(512 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("f.bin");

    let req = DownloadRequest::new(format!("http://{addr}/file"), out.clone()).connections(8);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("download should succeed");

    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
    // Work-stealing should have produced more segments than the 4 we started with.
    assert!(outcome.connections_used >= 4);
    // State sidecar removed on success.
    assert!(!DownloadState::state_path(&out).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_mirror_download_is_correct() {
    let data = make_data(300 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("m.bin");

    let req = DownloadRequest::new(format!("http://{addr}/file"), out.clone())
        .mirror(format!("http://{addr}/file"))
        .connections(6);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("mirror download should succeed");

    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_connection_fallback_without_ranges() {
    let data = make_data(128 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("s.bin");

    let req = DownloadRequest::new(format!("http://{addr}/noranges"), out.clone()).connections(8);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("single-connection download should succeed");

    assert_eq!(outcome.connections_used, 1);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_completes_and_matches() {
    let data = make_data(256 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("r.bin");
    let url = format!("http://{addr}/file");
    let total = data.len() as u64;
    let half = total / 2;

    // Simulate an interrupted download: first half already on disk, with a
    // matching state file marking segment 0 complete and segment 1 pending.
    preallocate(&out, total).unwrap();
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).open(&out).unwrap();
        f.write_all(&data[..half as usize]).unwrap();
    }
    let st = DownloadState {
        version: 1,
        urls: vec![url.clone()],
        output: out.clone(),
        total_size: Some(total),
        etag: None,
        last_modified: None,
        supports_ranges: true,
        connections: 4,
        segments: vec![
            SegmentState { index: 0, start: 0, end: half - 1, downloaded: half },
            SegmentState { index: 1, start: half, end: total - 1, downloaded: 0 },
        ],
    };
    st.save(&DownloadState::state_path(&out)).unwrap();

    let req = DownloadRequest::new(url, out.clone()).connections(4);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("resume should succeed");

    assert!(outcome.resumed, "download should have resumed from state");
    assert_eq!(std::fs::read(&out).unwrap(), data);
    assert!(!DownloadState::state_path(&out).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checksum_mismatch_is_reported() {
    let data = make_data(64 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("c.bin");

    let mut req = DownloadRequest::new(format!("http://{addr}/file"), out.clone()).connections(4);
    req.md5 = Some("00000000000000000000000000000000".into());

    let err = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect_err("should fail checksum");
    assert!(matches!(err, Error::Checksum { .. }), "got {err:?}");
}
