//! End-to-end tests against a local axum server with Range support.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use futures::stream::unfold;

use jyroxa_core::download::{preallocate, Control};
use jyroxa_core::events::{self, ProgressEvent};
use jyroxa_core::{
    DownloadConfig, DownloadRequest, DownloadState, Downloader, Error, SegmentState,
};

#[derive(Clone)]
struct AppState {
    data: Arc<Vec<u8>>,
    etag: String,
}

#[derive(Clone)]
struct FlakyState {
    data: Arc<Vec<u8>>,
    etag: String,
    responses: Arc<AtomicUsize>,
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

/// Serves range data but intermittently truncates the body to force mid-stream EOF.
async fn serve_flaky(State(st): State<FlakyState>, headers: HeaderMap) -> Response {
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let total = st.data.len() as u64;
    let slice = if let Some(r) = range {
        let Some((s, e)) = parse_range(r, total) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .body(Body::empty())
                .unwrap();
        };
        st.data[s as usize..=e as usize].to_vec()
    } else {
        st.data.to_vec()
    };

    let truncate = st.responses.fetch_add(1, Ordering::Relaxed) % 2 == 0;
    let send_len = if truncate && slice.len() > 128 {
        slice.len() / 3
    } else {
        slice.len()
    };

    if let Some(r) = range {
        if let Some((s, e)) = parse_range(r, total) {
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, slice.len().to_string())
                .header(header::ETAG, &st.etag)
                .body(Body::from(slice[..send_len].to_vec()))
                .unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, slice.len().to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, &st.etag)
        .body(Body::from(slice[..send_len].to_vec()))
        .unwrap()
}

async fn spawn_flaky_server(data: Vec<u8>) -> SocketAddr {
    let st = FlakyState {
        data: Arc::new(data),
        etag: "\"flaky\"".into(),
        responses: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/flaky", get(serve_flaky))
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
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

    let mut req = DownloadRequest::new(format!("http://{addr}/file"), out.clone()).connections(8);
    req.autotune = false;
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("download should succeed");

    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
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
            SegmentState {
                index: 0,
                start: 0,
                end: half - 1,
                downloaded: half,
            },
            SegmentState {
                index: 1,
                start: half,
                end: total - 1,
                downloaded: 0,
            },
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
async fn midstream_disconnects_retry_to_correct_checksum() {
    let data = make_data(400 * 1024);
    let sha = format!("{:x}", Sha256::digest(&data));
    let addr = spawn_flaky_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("flaky.bin");

    let mut req = DownloadRequest::new(format!("http://{addr}/flaky"), out.clone()).connections(8);
    req.sha256 = Some(sha);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("flaky download should eventually complete");

    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[derive(Clone)]
struct ThrottleState {
    data: Arc<Vec<u8>>,
    hits: Arc<AtomicUsize>,
}

async fn serve_throttle(State(st): State<ThrottleState>, headers: HeaderMap) -> Response {
    let n = st.hits.fetch_add(1, Ordering::Relaxed);
    if n % 3 == 0 {
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(header::RETRY_AFTER, "1")
            .body(Body::empty())
            .unwrap();
    }
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    build_response(&st.data, "\"throttle\"", range, true)
}

async fn spawn_throttle_server(data: Vec<u8>) -> SocketAddr {
    let st = ThrottleState {
        data: Arc::new(data),
        hits: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/throttle", get(serve_throttle))
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_stops_writes_quickly() {
    let data = make_data(2 * 1024 * 1024);
    let addr = spawn_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("pause.bin");
    let control = Control::new();
    let mut req = DownloadRequest::new(format!("http://{addr}/file"), out.clone()).connections(4);
    req.autotune = false;
    req.speed_limit = Some(200_000); // 200 KiB/s so pause is observable
    let engine = test_engine();
    let (tx, mut rx) = events::channel();
    let mut dl = Box::pin(engine.download(req, Some(tx), control.clone(), None));
    let mut last = 0u64;
    loop {
        tokio::select! {
            ev = rx.recv() => {
                let Some(ev) = ev else { break; };
                if let ProgressEvent::Progress { downloaded, .. } = ev {
                    last = downloaded;
                    if downloaded > 64 * 1024 {
                        break;
                    }
                }
            }
            _ = dl.as_mut() => panic!("download finished before pause test"),
        }
    }
    assert!(last > 0, "expected some progress before pause");
    control.pause();
    let at_pause = last;
    tokio::time::sleep(Duration::from_millis(300)).await;
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
        if let ProgressEvent::Progress { downloaded, .. } = ev {
            last = downloaded;
        }
    }
    control.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(10), dl.as_mut()).await;
    assert!(
        last.saturating_sub(at_pause) < 32 * 1024,
        "expected pause to limit growth, grew by {}",
        last.saturating_sub(at_pause)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_removes_partial_and_state() {
    let data = make_data(4 * 1024 * 1024);
    let addr = spawn_server(data).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cancel.bin");
    let control = Control::new();
    let mut req = DownloadRequest::new(format!("http://{addr}/file"), out.clone()).connections(4);
    req.autotune = false;
    req.speed_limit = Some(150_000);
    let engine = test_engine();
    let (tx, mut rx) = events::channel();
    let mut dl = Box::pin(engine.download(req, Some(tx), control.clone(), None));
    loop {
        tokio::select! {
            ev = rx.recv() => {
                let Some(ev) = ev else { panic!("channel closed before start"); };
                if let ProgressEvent::Progress { downloaded, .. } = ev {
                    if downloaded > 32 * 1024 {
                        break;
                    }
                }
            }
            r = dl.as_mut() => panic!("download finished early: {r:?}"),
        }
    }
    control.cancel();
    let result = tokio::time::timeout(Duration::from_secs(15), dl.as_mut())
        .await
        .expect("cancel should finish")
        .expect_err("expected cancellation");
    assert!(matches!(result, Error::Cancelled));
    assert!(!out.exists());
    assert!(!DownloadState::state_path(&out).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn throttle_429_download_completes() {
    let data = make_data(128 * 1024);
    let addr = spawn_throttle_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("throttle.bin");
    let req = DownloadRequest::new(format!("http://{addr}/throttle"), out.clone()).connections(4);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("should complete despite 429s");
    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[derive(Clone)]
struct SlowState {
    data: Arc<Vec<u8>>,
}

async fn serve_slow(State(st): State<SlowState>, headers: HeaderMap) -> Response {
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let total = st.data.len() as u64;
    let slice = if let Some(r) = range {
        let Some((s, e)) = parse_range(r, total) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .body(Body::empty())
                .unwrap();
        };
        st.data[s as usize..=e as usize].to_vec()
    } else {
        st.data.to_vec()
    };

    let len = slice.len();
    let buf = slice;
    let stream = unfold((buf, 0usize), |(data, off)| async move {
        if off >= data.len() {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
        let end = (off + 64 * 1024).min(data.len());
        let chunk = Bytes::copy_from_slice(&data[off..end]);
        Some((Ok::<Bytes, std::convert::Infallible>(chunk), (data, end)))
    });
    let body = Body::from_stream(stream);

    if let Some(r) = range {
        if let Some((s, e)) = parse_range(r, total) {
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::ETAG, "\"slow\"")
                .body(body)
                .unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, len.to_string())
        .header(header::ETAG, "\"slow\"")
        .body(body)
        .unwrap()
}

async fn spawn_slow_server(data: Vec<u8>) -> SocketAddr {
    let st = SlowState {
        data: Arc::new(data),
    };
    let app = Router::new().route("/slow", get(serve_slow)).with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[derive(Clone)]
struct IgnoreRangeState {
    data: Arc<Vec<u8>>,
}

async fn serve_ignore_range_head(State(st): State<IgnoreRangeState>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, st.data.len().to_string())
        .header(header::ETAG, "\"ignore\"")
        .body(Body::empty())
        .unwrap()
}

async fn serve_ignore_range(State(st): State<IgnoreRangeState>, _headers: HeaderMap) -> Response {
    build_response(&st.data, "\"ignore\"", None, false)
}

async fn spawn_ignore_range_server(data: Vec<u8>) -> SocketAddr {
    let st = IgnoreRangeState {
        data: Arc::new(data),
    };
    let app = Router::new()
        .route(
            "/ignore",
            get(serve_ignore_range).head(serve_ignore_range_head),
        )
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_stream_survives_read_timeout_not_total_deadline() {
    let data = make_data(4 * 1024 * 1024);
    let addr = spawn_slow_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("slow.bin");

    let config = DownloadConfig {
        read_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(1),
        min_segment_size: 64 * 1024,
        tune_interval: Duration::from_millis(100),
        ..DownloadConfig::default()
    };
    let engine = Downloader::new(config).unwrap();
    let req = DownloadRequest::new(format!("http://{addr}/slow"), out.clone()).connections(2);
    let outcome = engine
        .download(req, None, Control::new(), None)
        .await
        .expect("slow but steady download should complete");
    assert_eq!(outcome.total_size, data.len() as u64);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn range_ignored_falls_back_to_single_connection() {
    let data = make_data(200 * 1024);
    let addr = spawn_ignore_range_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("ignore.bin");

    // Probe thinks ranges work (HEAD advertises Accept-Ranges) but GET ignores Range.
    let req = DownloadRequest::new(format!("http://{addr}/ignore"), out.clone()).connections(8);
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("should fall back to single-connection download");

    assert_eq!(outcome.connections_used, 1);
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn throttle_429_autotune_stays_at_least_one_connection() {
    let data = make_data(128 * 1024);
    let addr = spawn_throttle_server(data.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("throttle_auto.bin");
    let mut req =
        DownloadRequest::new(format!("http://{addr}/throttle"), out.clone()).connections(8);
    req.autotune = true;
    let outcome = test_engine()
        .download(req, None, Control::new(), None)
        .await
        .expect("should complete despite 429s");
    assert!(outcome.connections_used >= 1);
    assert_eq!(outcome.total_size, data.len() as u64);
}

/// Per-connection rate cap with one randomly stalled connection; multi-conn
/// must beat single-conn by a wide margin and recover from stalls quickly.
#[derive(Clone)]
struct RateStallState {
    data: Arc<Vec<u8>>,
    conns: Arc<AtomicUsize>,
}

async fn serve_rate_stall(State(st): State<RateStallState>, headers: HeaderMap) -> Response {
    let conn = st.conns.fetch_add(1, Ordering::Relaxed);
    // Exactly one bad connection (the first) simulates a randomly stalled link.
    let stall = conn == 0;
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let total = st.data.len() as u64;
    let slice = if let Some(r) = range {
        let Some((s, e)) = parse_range(r, total) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .body(Body::empty())
                .unwrap();
        };
        st.data[s as usize..=e as usize].to_vec()
    } else {
        st.data.to_vec()
    };

    // ~2 MiB/s per connection: 256 KiB every 125 ms.
    let body_len = slice.len();
    let stream = unfold(
        (slice, 0usize, stall),
        move |(data, off, stall)| async move {
            let pace = Duration::from_millis(125);
            let chunk = 256 * 1024usize;
            if off >= data.len() {
                return None;
            }
            if stall && off > data.len() / 4 {
                // Stall mid-body indefinitely until the client gives up and re-steals.
                tokio::time::sleep(Duration::from_secs(60)).await;
                return None;
            }
            tokio::time::sleep(pace).await;
            let end = (off + chunk).min(data.len());
            let piece = Bytes::copy_from_slice(&data[off..end]);
            Some((
                Ok::<Bytes, std::convert::Infallible>(piece),
                (data, end, stall),
            ))
        },
    );
    let body = Body::from_stream(stream);

    if let Some(r) = range {
        if let Some((s, e)) = parse_range(r, total) {
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, body_len.to_string())
                .header(header::ETAG, "\"rate-stall\"")
                .body(body)
                .unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, body_len.to_string())
        .header(header::ETAG, "\"rate-stall\"")
        .body(body)
        .unwrap()
}

async fn spawn_rate_stall_server(data: Vec<u8>) -> SocketAddr {
    let st = RateStallState {
        data: Arc::new(data),
        conns: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/rate-stall", get(serve_rate_stall))
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_connection_beats_one_under_per_conn_rate_limit_and_stall() {
    let data = make_data(8 * 1024 * 1024);
    let addr = spawn_rate_stall_server(data.clone()).await;
    let url = format!("http://{addr}/rate-stall");
    let dir = tempfile::tempdir().unwrap();

    let config = DownloadConfig {
        min_segment_size: 64 * 1024,
        max_connections: 8,
        tune_interval: Duration::from_millis(50),
        read_timeout: Duration::from_secs(30),
        ..DownloadConfig::default()
    };

    let one = dir.path().join("one.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config.clone()).unwrap();
    let req1 = DownloadRequest::new(url.clone(), one.clone()).connections(1);
    eng.download(req1, None, Control::new(), None)
        .await
        .expect("1-conn");
    let t1 = t0.elapsed();

    let eight = dir.path().join("eight.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config).unwrap();
    let mut req8 = DownloadRequest::new(url, eight.clone()).connections(8);
    req8.autotune = false;
    eng.download(req8, None, Control::new(), None)
        .await
        .expect("8-conn");
    let t8 = t0.elapsed();

    assert_eq!(std::fs::read(&eight).unwrap(), data);
    assert_eq!(std::fs::read(&one).unwrap(), data);
    // 8 connections on ~2 MiB/s per link should finish much faster than 1.
    assert!(
        t8 < t1 / 2,
        "8-conn {:?} should be < half of 1-conn {:?}",
        t8,
        t1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn autotune_matches_single_connection_on_unlimited_server() {
    let data = make_data(12 * 1024 * 1024);
    let addr = spawn_server(data.clone()).await;
    let url = format!("http://{addr}/file");
    let dir = tempfile::tempdir().unwrap();

    let config = DownloadConfig {
        min_segment_size: 64 * 1024,
        max_connections: 16,
        tune_interval: Duration::from_millis(50),
        ..DownloadConfig::default()
    };

    let one = dir.path().join("one.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config.clone()).unwrap();
    let mut req1 = DownloadRequest::new(url.clone(), one.clone()).connections(1);
    req1.autotune = false;
    eng.download(req1, None, Control::new(), None)
        .await
        .expect("1-conn");
    let t1 = t0.elapsed();

    let auto = dir.path().join("auto.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config).unwrap();
    let mut req_auto = DownloadRequest::new(url, auto.clone()).connections(16);
    req_auto.autotune = true;
    eng.download(req_auto, None, Control::new(), None)
        .await
        .expect("auto");
    let t_auto = t0.elapsed();

    assert_eq!(std::fs::read(&auto).unwrap(), data);
    let bps_one = data.len() as f64 / t1.as_secs_f64();
    let bps_auto = data.len() as f64 / t_auto.as_secs_f64();
    assert!(
        bps_auto >= bps_one * 0.97,
        "auto {bps_auto} B/s should be >= 97% of 1-conn {bps_one} B/s (wall {:?} vs {:?})",
        t_auto,
        t1
    );
}

#[derive(Clone)]
struct PerConnCapState {
    data: Arc<Vec<u8>>,
}

async fn serve_per_conn_cap(State(st): State<PerConnCapState>, headers: HeaderMap) -> Response {
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let total = st.data.len() as u64;
    let slice = if let Some(r) = range {
        let Some((s, e)) = parse_range(r, total) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .body(Body::empty())
                .unwrap();
        };
        st.data[s as usize..=e as usize].to_vec()
    } else {
        st.data.to_vec()
    };

    // ~200 KiB/s per TCP connection: 32 KiB every 160 ms.
    let body_len = slice.len();
    let stream = unfold(slice, move |data| async move {
        let pace = Duration::from_millis(160);
        let chunk = 32 * 1024usize;
        if data.is_empty() {
            return None;
        }
        tokio::time::sleep(pace).await;
        let end = chunk.min(data.len());
        let piece = Bytes::copy_from_slice(&data[..end]);
        let rest = data[end..].to_vec();
        Some((Ok::<Bytes, std::convert::Infallible>(piece), rest))
    });
    let body = Body::from_stream(stream);

    if let Some(r) = range {
        if let Some((s, e)) = parse_range(r, total) {
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, body_len.to_string())
                .body(body)
                .unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, body_len.to_string())
        .body(body)
        .unwrap()
}

async fn spawn_per_conn_cap_server(data: Vec<u8>) -> SocketAddr {
    let st = PerConnCapState {
        data: Arc::new(data),
    };
    let app = Router::new()
        .route("/cap", get(serve_per_conn_cap))
        .with_state(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn autotune_scales_on_per_connection_rate_cap() {
    let data = make_data(12 * 1024 * 1024);
    let addr = spawn_per_conn_cap_server(data.clone()).await;
    let url = format!("http://{addr}/cap");
    let dir = tempfile::tempdir().unwrap();

    let config = DownloadConfig {
        min_segment_size: 64 * 1024,
        max_connections: 8,
        tune_interval: Duration::from_millis(50),
        ..DownloadConfig::default()
    };

    let one = dir.path().join("one.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config.clone()).unwrap();
    let mut req1 = DownloadRequest::new(url.clone(), one.clone()).connections(1);
    req1.autotune = false;
    eng.download(req1, None, Control::new(), None)
        .await
        .expect("1-conn");
    let t1 = t0.elapsed();

    let auto = dir.path().join("auto.bin");
    let t0 = std::time::Instant::now();
    let eng = Downloader::new(config).unwrap();
    let mut req_auto = DownloadRequest::new(url, auto.clone()).connections(8);
    req_auto.autotune = true;
    let outcome = eng
        .download(req_auto, None, Control::new(), None)
        .await
        .expect("auto");
    let t_auto = t0.elapsed();

    assert_eq!(std::fs::read(&auto).unwrap(), data);
    let bps_one = data.len() as f64 / t1.as_secs_f64();
    let bps_auto = data.len() as f64 / t_auto.as_secs_f64();
    assert!(
        outcome.connections_used >= 2,
        "auto should scale past 1 connection on per-conn caps, used {}",
        outcome.connections_used
    );
    assert!(
        bps_auto >= bps_one * 1.9,
        "auto {bps_auto} B/s ({} conns) should clearly beat 1-conn {bps_one} B/s",
        outcome.connections_used,
    );
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
