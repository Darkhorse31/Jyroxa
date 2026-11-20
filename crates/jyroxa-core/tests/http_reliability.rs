//! Adversarial HTTP responses must recover or fail within a bounded time.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use futures::stream;
use jyroxa_core::{Control, DownloadConfig, DownloadRequest, Downloader};

const SIZE: u64 = 4 * 1024 * 1024;

#[test]
fn control_signals_before_first_subscriber_are_retained() {
    let control = Control::new();
    control.pause();
    assert!(control.is_paused());
    control.resume();
    assert!(!control.is_paused());
    control.cancel();
    assert!(control.is_cancelled());
}

#[derive(Clone, Copy)]
enum Behavior {
    Throttle(u16),
    Empty,
    WrongRange,
    SlowHead,
    StallOnce,
    StallMetadata,
    EmptyTail,
    Expired,
}

#[derive(Clone)]
struct ServerState {
    behavior: Behavior,
    requests: Arc<AtomicUsize>,
}

async fn head(State(state): State<ServerState>) -> Response {
    if matches!(state.behavior, Behavior::SlowHead) {
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
    Response::builder()
        .header(header::CONTENT_LENGTH, SIZE)
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .unwrap()
}

async fn serve(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let range = headers[header::RANGE]
        .to_str()
        .unwrap()
        .strip_prefix("bytes=")
        .unwrap();
    let (first, last) = range.split_once('-').unwrap();
    let start: u64 = first.parse().unwrap();
    let end: u64 = if last.is_empty() {
        SIZE - 1
    } else {
        last.parse().unwrap()
    };
    // Only the tiny bytes=0-0 metadata check is exempt; open-ended lead ranges misbehave.
    let metadata_probe = start == 0 && !last.is_empty() && last.parse::<u64>().ok() == Some(0);
    if metadata_probe && matches!(state.behavior, Behavior::StallMetadata) {
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
    let request = if metadata_probe {
        0
    } else {
        state.requests.fetch_add(1, Ordering::Relaxed)
    };
    if !metadata_probe {
        if matches!(state.behavior, Behavior::Expired) {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::empty())
                .unwrap();
        }
        if let Behavior::Throttle(status) = state.behavior {
            return Response::builder()
                .status(status)
                .header(header::RETRY_AFTER, "0")
                .body(Body::empty())
                .unwrap();
        }
    }
    let body = if !metadata_probe
        && (matches!(state.behavior, Behavior::Empty)
            || (matches!(state.behavior, Behavior::EmptyTail) && start >= SIZE - 1024))
    {
        // No Content-Length: EOF is legal at the HTTP layer, but not for this range.
        Body::empty()
    } else if !metadata_probe && matches!(state.behavior, Behavior::StallOnce) && request == 0 {
        let chunks = stream::unfold(false, |sent| async move {
            if sent {
                std::future::pending::<()>().await;
                None
            } else {
                Some((Ok::<_, std::io::Error>(Bytes::from(vec![71; 1024])), true))
            }
        });
        Body::from_stream(chunks)
    } else {
        let body_end = if !metadata_probe && matches!(state.behavior, Behavior::EmptyTail) {
            end.min(SIZE - 1025)
        } else {
            end
        };
        Body::from(vec![71; (body_end - start + 1) as usize])
    };
    let advertised_start = if !metadata_probe && matches!(state.behavior, Behavior::WrongRange) {
        start + 1
    } else {
        start
    };
    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(
            header::CONTENT_RANGE,
            format!("bytes {advertised_start}-{end}/{SIZE}"),
        )
        .body(body)
        .unwrap()
}

async fn server(behavior: Behavior) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let requests = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/file", get(serve).head(head))
        .with_state(ServerState {
            behavior,
            requests: requests.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, requests, task)
}

fn engine() -> Downloader {
    Downloader::new(DownloadConfig {
        max_retries: 1,
        max_global_failures: 100,
        tune_interval: Duration::from_millis(50),
        ..DownloadConfig::default()
    })
    .unwrap()
}

async fn must_fail(behavior: Behavior) {
    let (url, requests, server) = server(behavior).await;
    let dir = tempfile::tempdir().unwrap();
    let mut request = DownloadRequest::new(url, dir.path().join("file.bin")).connections(1);
    request.autotune = false;
    let control = Control::new();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        engine().download(request, None, control.clone(), None),
    )
    .await;
    control.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    server.abort();
    assert!(
        result.is_ok(),
        "download hung after {} requests",
        requests.load(Ordering::Relaxed)
    );
    assert!(result.unwrap().is_err(), "invalid response accepted");
    assert!(
        requests.load(Ordering::Relaxed) <= 3,
        "retry budget was reset"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn persistent_429_exhausts_retry_budget() {
    must_fail(Behavior::Throttle(429)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn persistent_503_exhausts_retry_budget() {
    must_fail(Behavior::Throttle(503)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_partial_response_exhausts_retry_budget() {
    must_fail(Behavior::Empty).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_content_range_is_rejected() {
    must_fail(Behavior::WrongRange).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_expired_link_refresh_exhausts_retry_budget() {
    let (url, requests, server) = server(Behavior::Expired).await;
    let dir = tempfile::tempdir().unwrap();
    let request = DownloadRequest::new(url, dir.path().join("expired.bin")).connections(1);
    let refresher: jyroxa_core::RefreshFn = Arc::new(|url| Box::pin(async move { Some(url) }));
    let control = Control::new();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        engine().download(request, None, control.clone(), Some(refresher)),
    )
    .await;
    control.cancel();
    server.abort();
    assert!(matches!(
        result,
        Ok(Err(jyroxa_core::Error::LinkExpired { .. }))
    ));
    assert!(
        requests.load(Ordering::Relaxed) <= 3,
        "lead stream adds one extra round trip before fallback"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_head_does_not_delay_parallel_start() {
    let (url, _, server) = server(Behavior::SlowHead).await;
    let dir = tempfile::tempdir().unwrap();
    let request = DownloadRequest::new(url, dir.path().join("file.bin")).connections(8);
    let start = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        engine().download(request, None, Control::new(), None),
    )
    .await;
    server.abort();
    assert!(
        result.is_ok(),
        "HEAD delayed startup for {:?}",
        start.elapsed()
    );
    result.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_connection_retries_from_received_cursor() {
    let (url, requests, server) = server(Behavior::StallOnce).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("file.bin");
    let mut request = DownloadRequest::new(url, &out).connections(1);
    request.autotune = false;
    let downloader = Downloader::new(DownloadConfig {
        read_timeout: Duration::from_millis(150),
        ..DownloadConfig::default()
    })
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        downloader.download(request, None, Control::new(), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(std::fs::read(out).unwrap(), vec![71; SIZE as usize]);
    assert!(requests.load(Ordering::Relaxed) >= 2);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancellation_interrupts_metadata_request() {
    let (url, _, server) = server(Behavior::StallMetadata).await;
    let dir = tempfile::tempdir().unwrap();
    let request = DownloadRequest::new(url, dir.path().join("file.bin"));
    let control = Control::new();
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        control.cancel();
    };
    let downloader = engine();
    let (result, ()) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            downloader.download(request, None, control.clone(), None)
        ),
        cancel
    );
    server.abort();
    assert!(matches!(result, Ok(Err(jyroxa_core::Error::Cancelled))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn healthy_mirror_starts_while_primary_metadata_is_stalled() {
    let (primary, _, primary_server) = server(Behavior::StallMetadata).await;
    let (mirror, _, mirror_server) = server(Behavior::SlowHead).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("mirror.bin");
    let request = DownloadRequest::new(primary, &out)
        .mirror(mirror)
        .connections(8);
    tokio::time::timeout(
        Duration::from_secs(3),
        engine().download(request, None, Control::new(), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(std::fs::read(out).unwrap(), vec![71; SIZE as usize]);
    primary_server.abort();
    mirror_server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_payload_is_not_resumed_from_complete_checkpoint() {
    let (url, _, server) = server(Behavior::SlowHead).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("missing.bin");
    let checkpoint = jyroxa_core::DownloadState {
        version: jyroxa_core::state::STATE_VERSION,
        urls: vec![url.clone()],
        output: out.clone(),
        total_size: Some(SIZE),
        etag: None,
        last_modified: None,
        supports_ranges: true,
        connections: 1,
        segments: vec![jyroxa_core::SegmentState {
            index: 0,
            start: 0,
            end: SIZE - 1,
            downloaded: SIZE,
        }],
    };
    checkpoint
        .save(&jyroxa_core::DownloadState::state_path(&out))
        .unwrap();
    let request = DownloadRequest::new(url, &out).connections(1);
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        engine().download(request, None, Control::new(), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!outcome.resumed);
    assert_eq!(std::fs::read(out).unwrap(), vec![71; SIZE as usize]);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn configured_connection_cap_is_honored() {
    let (url, _, server) = server(Behavior::SlowHead).await;
    let dir = tempfile::tempdir().unwrap();
    let request = DownloadRequest::new(url, dir.path().join("capped.bin")).connections(32);
    let downloader = Downloader::new(DownloadConfig {
        max_connections: 2,
        ..DownloadConfig::default()
    })
    .unwrap();
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        downloader.download(request, None, Control::new(), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(outcome.connections_used, 2);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_tail_at_99_percent_fails_without_hanging() {
    let (url, requests, server) = server(Behavior::EmptyTail).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tail.bin");
    let mut request = DownloadRequest::new(url, &out).connections(1);
    request.autotune = false;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        engine().download(request, None, Control::new(), None),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert_eq!(requests.load(Ordering::Relaxed), 3);
    let checkpoint = jyroxa_core::DownloadState::load_for_output(&out)
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.bytes_downloaded(), SIZE - 1024);
    server.abort();
}
