//! The multi-connection, multi-mirror, auto-tuning download engine.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::StreamExt;
use tokio::sync::{mpsc, watch, Notify};

use crate::checksum::{self, Algo};
use crate::error::{Error, Result};
use crate::events::{progress_tick, EventSender, ProgressEvent};
use crate::hoststats::HostStats;
use crate::limiter::RateLimiter;
use crate::segment::{split_initial, Segment, SegmentTable};
use crate::state::DownloadState;
use crate::trace::Trace;
use crate::tuner::{parse_retry_after, Tuner, TunerInput};

/// A callback that, given an expired URL, asynchronously returns a fresh one.
/// Used for links that return 403/410 after their signed-token expires.
pub type RefreshFn = Arc<dyn Fn(String) -> BoxFuture<'static, Option<String>> + Send + Sync>;

/// Mirrors with penalty at or above this threshold are treated as dead until all are.
const MIRROR_DEAD_PENALTY: i64 = 20;

// ---------------------------------------------------------------------------
// Control (pause / resume / cancel)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cmd {
    Run,
    Paused,
    Cancelled,
}

/// A clonable control handle for a running download.
#[derive(Clone)]
pub struct Control {
    tx: watch::Sender<Cmd>,
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

impl Control {
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(Cmd::Run);
        Control { tx }
    }
    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Paused);
    }
    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Run);
    }
    pub fn cancel(&self) {
        let _ = self.tx.send(Cmd::Cancelled);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.tx.borrow() == Cmd::Cancelled
    }
    pub fn is_paused(&self) -> bool {
        *self.tx.borrow() == Cmd::Paused
    }
    fn subscribe(&self) -> watch::Receiver<Cmd> {
        self.tx.subscribe()
    }
}

/// Block an async worker while paused; return `Err(Cancelled)` if cancelled.
async fn wait_runnable(rx: &mut watch::Receiver<Cmd>) -> Result<()> {
    loop {
        let cmd = *rx.borrow_and_update();
        match cmd {
            Cmd::Cancelled => return Err(Error::Cancelled),
            Cmd::Run => return Ok(()),
            Cmd::Paused => {
                if rx.changed().await.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

/// Acquire rate tokens in small steps that respect pause/cancel.
async fn acquire_rate(
    limiter: &RateLimiter,
    global: &RateLimiter,
    nbytes: u64,
    ctrl_rx: &mut watch::Receiver<Cmd>,
) -> Result<()> {
    let mut left = nbytes;
    let poll_rx = ctrl_rx.clone();
    let still_running = || matches!(*poll_rx.borrow(), Cmd::Run | Cmd::Paused);
    while left > 0 {
        wait_runnable(ctrl_rx).await?;
        let step = left.min(32 * 1024);
        limiter.acquire_until(step, still_running).await;
        if matches!(*ctrl_rx.borrow(), Cmd::Cancelled) {
            return Err(Error::Cancelled);
        }
        global.acquire_until(step, still_running).await;
        if matches!(*ctrl_rx.borrow(), Cmd::Cancelled) {
            return Err(Error::Cancelled);
        }
        left -= step;
    }
    Ok(())
}

/// Sleep that wakes promptly on pause, resume, or cancel.
async fn interruptible_sleep(rx: &mut watch::Receiver<Cmd>, dur: Duration) -> Result<()> {
    if dur.is_zero() {
        return wait_runnable(rx).await;
    }
    tokio::select! {
        res = rx.changed() => {
            if res.is_err() {
                return Ok(());
            }
            wait_runnable(rx).await
        }
        _ = tokio::time::sleep(dur) => wait_runnable(rx).await,
    }
}

const SEGMENT_WRITE_BUF: usize = 2 * 1024 * 1024;
const RETRY_AFTER_CAP: Duration = Duration::from_secs(30);
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(250);
const SEGMENT_STALL_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound for the metadata HEAD; the ranged GET fallback covers servers that stall on HEAD.
pub(crate) const HEAD_TIMEOUT: Duration = Duration::from_secs(8);

// ---------------------------------------------------------------------------
// Configuration and request/outcome types
// ---------------------------------------------------------------------------

/// Tunable engine parameters.
#[derive(Clone, Debug)]
pub struct DownloadConfig {
    /// Hard cap on simultaneous connections (will be clamped to [`MAX_CONNECTIONS`]).
    pub max_connections: usize,
    /// Starting connection count when auto-tuning.
    pub initial_connections: usize,
    /// Smallest segment the splitter/stealer will create.
    pub min_segment_size: u64,
    /// Max retries per segment before it is considered failed.
    pub max_retries: usize,
    /// Segment exhaustion events allowed across all workers before the download fails.
    pub max_global_failures: usize,
    /// Enable per-host connection auto-tuning.
    pub autotune: bool,
    /// How often to sample throughput / tune / persist state.
    pub tune_interval: Duration,
    /// Total timeout for small metadata probes (HEAD / tiny ranged GET).
    pub request_timeout: Duration,
    /// Idle timeout while waiting for the next chunk on an open response body.
    pub read_timeout: Duration,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        DownloadConfig {
            max_connections: crate::MAX_CONNECTIONS,
            initial_connections: 1,
            min_segment_size: 2 << 20, // 2 MiB
            max_retries: 5,
            max_global_failures: 0,
            autotune: true,
            tune_interval: Duration::from_millis(500),
            request_timeout: Duration::from_secs(60),
            read_timeout: crate::DEFAULT_READ_TIMEOUT,
        }
    }
}

/// A download request: one logical file, one or more mirror URLs.
#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub urls: Vec<String>,
    pub output: PathBuf,
    /// Desired maximum connections for this download.
    pub connections: usize,
    pub speed_limit: Option<u64>,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub autotune: bool,
}

impl DownloadRequest {
    pub fn new(url: impl Into<String>, output: impl Into<PathBuf>) -> Self {
        DownloadRequest {
            urls: vec![url.into()],
            output: output.into(),
            connections: 8,
            speed_limit: None,
            sha256: None,
            md5: None,
            autotune: true,
        }
    }
    pub fn mirror(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }
    pub fn connections(mut self, n: usize) -> Self {
        self.connections = n;
        self
    }
    pub fn speed_limit(mut self, bps: Option<u64>) -> Self {
        self.speed_limit = bps;
        self
    }
}

/// The result of a completed download.
#[derive(Clone, Debug)]
pub struct DownloadOutcome {
    pub output: PathBuf,
    pub total_size: u64,
    pub elapsed: Duration,
    pub average_bps: f64,
    pub connections_used: usize,
    pub resumed: bool,
}

// ---------------------------------------------------------------------------
// Metadata probe
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Meta {
    size: Option<u64>,
    supports_ranges: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    filename: Option<String>,
    final_url: String,
}

fn header_str(h: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    h.get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Parse `Content-Disposition` for a filename.
pub fn filename_from_disposition(value: &str) -> Option<String> {
    crate::naming::filename_from_disposition(value).map(|s| crate::naming::sanitize_filename(&s))
}

/// Strip path separators / parent references from a candidate filename.
pub fn sanitize_name(name: &str) -> String {
    crate::naming::sanitize_filename(name)
}

/// Derive a filename from a URL path, if any.
pub fn filename_from_url(u: &url::Url) -> Option<String> {
    crate::naming::filename_from_url_path(u).map(|s| crate::naming::sanitize_filename(&s))
}

async fn probe(client: &reqwest::Client, url: &str, timeout: Duration) -> Result<Meta> {
    match probe_metadata(client, url, timeout).await {
        Ok(m) => Ok(m),
        Err(_) => probe_streaming_fallback(client, url, timeout).await,
    }
}

async fn probe_metadata(client: &reqwest::Client, url: &str, timeout: Duration) -> Result<Meta> {
    use reqwest::header as h;
    let mut meta = Meta {
        final_url: url.to_string(),
        ..Default::default()
    };

    // 1. HEAD for size / validators / filename.
    // Some servers never answer HEAD (the request just hangs); cap it so the
    // ranged GET below gets a turn instead of the task sitting at Starting.
    if let Ok(resp) = client.head(url).timeout(timeout.min(HEAD_TIMEOUT)).send().await {
        if resp.status().is_success() {
            let headers = resp.headers().clone();
            meta.final_url = resp.url().to_string();
            // A HEAD often reports Content-Length: 0 (e.g. servers that strip the
            // body length, or frameworks that recompute it from an empty body).
            // Treat a zero length as "unknown" so the ranged GET below confirms
            // the real size instead of us wrongly concluding the file is empty.
            meta.size = resp.content_length().filter(|&n| n > 0);
            meta.etag = header_str(&headers, h::ETAG);
            meta.last_modified = header_str(&headers, h::LAST_MODIFIED);
            meta.supports_ranges = header_str(&headers, h::ACCEPT_RANGES)
                .map(|v| v.to_ascii_lowercase().contains("bytes"))
                .unwrap_or(false);
            if let Some(cd) = header_str(&headers, h::CONTENT_DISPOSITION) {
                meta.filename = filename_from_disposition(&cd);
            }
        }
    }

    // 2. Confirm range support / size with a tiny ranged GET when needed.
    if !meta.supports_ranges || meta.size.is_none() {
        let resp = client
            .get(&meta.final_url)
            .header(h::RANGE, "bytes=0-0")
            .timeout(timeout)
            .send()
            .await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        meta.final_url = resp.url().to_string();
        if meta.etag.is_none() {
            meta.etag = header_str(&headers, h::ETAG);
        }
        if meta.last_modified.is_none() {
            meta.last_modified = header_str(&headers, h::LAST_MODIFIED);
        }
        if meta.filename.is_none() {
            if let Some(cd) = header_str(&headers, h::CONTENT_DISPOSITION) {
                meta.filename = filename_from_disposition(&cd);
            }
        }
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            meta.supports_ranges = true;
            // Content-Range: bytes 0-0/12345
            if let Some(cr) = header_str(&headers, h::CONTENT_RANGE) {
                if let Some(total) = cr
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    meta.size = Some(total);
                }
            }
        } else if status.is_success() {
            meta.supports_ranges = false;
            if let Some(len) = resp.content_length() {
                meta.size = Some(len);
            }
        } else {
            return Err(Error::Status(status.as_u16()));
        }
    }

    Ok(meta)
}

/// Last-resort probe: plain GET (no Range) for hosts that reject HEAD/Range checks.
async fn probe_streaming_fallback(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Meta> {
    use reqwest::header as h;
    let resp = client.get(url).timeout(timeout).send().await?;
    if !resp.status().is_success() {
        return Err(Error::Status(resp.status().as_u16()));
    }
    let headers = resp.headers().clone();
    let mut meta = Meta {
        final_url: resp.url().to_string(),
        supports_ranges: false,
        size: resp.content_length().filter(|&n| n > 0),
        etag: header_str(&headers, h::ETAG),
        last_modified: header_str(&headers, h::LAST_MODIFIED),
        filename: header_str(&headers, h::CONTENT_DISPOSITION)
            .and_then(|cd| filename_from_disposition(&cd)),
    };
    if meta.size.is_none() {
        if let Some(accept) = header_str(&headers, h::ACCEPT_RANGES) {
            meta.supports_ranges = accept.to_ascii_lowercase().contains("bytes");
        }
    }
    Ok(meta)
}

// ---------------------------------------------------------------------------
// Mirror pool
// ---------------------------------------------------------------------------

struct Mirror {
    url: Mutex<String>,
    /// Higher = worse. Successes decay it, failures raise it.
    penalty: AtomicI64,
}

struct MirrorPool {
    mirrors: Vec<Arc<Mirror>>,
    rr: AtomicUsize,
    events: Option<EventSender>,
}

impl MirrorPool {
    fn new(urls: &[String], events: Option<EventSender>) -> Self {
        MirrorPool {
            mirrors: urls
                .iter()
                .map(|u| {
                    Arc::new(Mirror {
                        url: Mutex::new(u.clone()),
                        penalty: AtomicI64::new(0),
                    })
                })
                .collect(),
            rr: AtomicUsize::new(0),
            events,
        }
    }

    /// Pick a mirror, preferring those with the lowest penalty, round-robin.
    fn pick(&self) -> Arc<Mirror> {
        let n = self.mirrors.len();
        if n == 0 {
            unreachable!("mirror pool is never empty");
        }
        let viable: Vec<&Arc<Mirror>> = self
            .mirrors
            .iter()
            .filter(|m| m.penalty.load(Ordering::Relaxed) < MIRROR_DEAD_PENALTY)
            .collect();
        let pool: Vec<&Arc<Mirror>> = if viable.is_empty() {
            self.mirrors.iter().collect()
        } else {
            viable
        };
        let min = pool
            .iter()
            .map(|m| m.penalty.load(Ordering::Relaxed))
            .min()
            .unwrap_or(0);
        let pn = pool.len();
        for _ in 0..pn {
            let i = self.rr.fetch_add(1, Ordering::Relaxed) % pn;
            if pool[i].penalty.load(Ordering::Relaxed) <= min + 1 {
                return pool[i].clone();
            }
        }
        let i = self.rr.fetch_add(1, Ordering::Relaxed) % pn;
        pool[i].clone()
    }

    fn url_of(&self, m: &Mirror) -> String {
        m.url.lock().unwrap().clone()
    }

    fn reward(&self, m: &Mirror) {
        let p = m.penalty.load(Ordering::Relaxed);
        if p > 0 {
            m.penalty.store(p - 1, Ordering::Relaxed);
        }
    }

    fn demote(&self, m: &Mirror) {
        m.penalty.fetch_add(2, Ordering::Relaxed);
        if let Some(tx) = &self.events {
            let _ = tx.send(ProgressEvent::MirrorDemoted {
                url: self.url_of(m),
            });
        }
    }

    fn refresh(&self, m: &Mirror, new_url: String) {
        let old = {
            let mut u = m.url.lock().unwrap();
            let old = u.clone();
            *u = new_url.clone();
            old
        };
        m.penalty.store(0, Ordering::Relaxed);
        if let Some(tx) = &self.events {
            let _ = tx.send(ProgressEvent::LinkRefreshed { old, new: new_url });
        }
    }
}

// ---------------------------------------------------------------------------
// Positional writes (cross-platform)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn pwrite_all(file: &std::fs::File, mut offset: u64, mut buf: &[u8]) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_write(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "seek_write wrote 0 bytes",
            ));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn pwrite_all(file: &std::fs::File, mut offset: u64, mut buf: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    while !buf.is_empty() {
        let n = file.write_at(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "write_at wrote 0 bytes",
            ));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared download context
// ---------------------------------------------------------------------------

struct WriteChunk {
    offset: u64,
    data: Vec<u8>,
    seg: Arc<Segment>,
}

struct Ctx {
    writer: Mutex<Option<mpsc::Sender<WriteChunk>>>,
    pool: MirrorPool,
    path: PathBuf,
    total: u64,
    table: SegmentTable,
    downloaded: AtomicU64,
    write_pending: Arc<AtomicUsize>,
    limiter: RateLimiter,
    global: Arc<RateLimiter>,
    control: Control,
    events: Option<EventSender>,
    refresher: Option<RefreshFn>,
    config: DownloadConfig,
    active_workers: AtomicUsize,
    /// Supervisor's desired worker count (workers retire when active > target).
    target: AtomicUsize,
    recent_errors: AtomicU64,
    recent_throttles: AtomicU64,
    global_fail_streak: AtomicUsize,
    global_fail_bound: usize,
    fatal: Arc<Mutex<Option<Error>>>,
    notify: Notify,
    final_connections: AtomicUsize,
    busy_workers: AtomicUsize,
    range_fallback: AtomicBool,
    last_progress_emit: Mutex<Instant>,
    trace: Mutex<Trace>,
}

fn spawn_disk_writer(
    path: PathBuf,
    fatal: Arc<Mutex<Option<Error>>>,
    pending: Arc<AtomicUsize>,
) -> mpsc::Sender<WriteChunk> {
    let (tx, mut rx) = mpsc::channel::<WriteChunk>(64);
    tokio::spawn(async move {
        let file = match std::fs::OpenOptions::new().write(true).open(&path) {
            Ok(f) => f,
            Err(e) => {
                let mut slot = fatal.lock().unwrap();
                if slot.is_none() {
                    *slot = Some(Error::other(e.to_string()));
                }
                return;
            }
        };
        while let Some(chunk) = rx.recv().await {
            let off = chunk.offset;
            let data = chunk.data;
            let len = data.len();
            let seg = chunk.seg;
            let out = match file.try_clone() {
                Ok(f) => f,
                Err(e) => {
                    let mut slot = fatal.lock().unwrap();
                    if slot.is_none() {
                        *slot = Some(Error::other(e.to_string()));
                    }
                    break;
                }
            };
            let written = tokio::task::spawn_blocking(move || pwrite_all(&out, off, &data)).await;
            pending.fetch_sub(1, Ordering::AcqRel);
            match written {
                Ok(Ok(())) => {
                    let end = off + len as u64;
                    seg.current.store(end, Ordering::Release);
                }
                Ok(Err(e)) => {
                    let mut slot = fatal.lock().unwrap();
                    if slot.is_none() {
                        *slot = Some(Error::other(e.to_string()));
                    }
                    break;
                }
                Err(e) => {
                    let mut slot = fatal.lock().unwrap();
                    if slot.is_none() {
                        *slot = Some(Error::other(e.to_string()));
                    }
                    break;
                }
            }
        }
    });
    tx
}

async fn drain_disk_writes(ctx: &Ctx) {
    for _ in 0..10_000 {
        if ctx.write_pending.load(Ordering::Acquire) == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

impl Ctx {
    fn emit(&self, ev: ProgressEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(ev);
        }
    }

    fn set_fatal(&self, err: Error) {
        let mut slot = self.fatal.lock().unwrap();
        if slot.is_none() {
            *slot = Some(err);
        }
        self.control.cancel();
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// The download engine. Cheap to clone-share via the owned client.
#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
    global_limiter: Arc<RateLimiter>,
    host_stats: Arc<HostStats>,
    config: DownloadConfig,
}

impl Downloader {
    /// Build an engine with a fresh client and the given config.
    pub fn new(config: DownloadConfig) -> Result<Self> {
        Ok(Downloader {
            client: crate::build_client_with(config.read_timeout)?,
            global_limiter: Arc::new(RateLimiter::unlimited()),
            host_stats: Arc::new(HostStats::in_memory()),
            config,
        })
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }
    pub fn with_global_limit(mut self, bytes_per_sec: Option<u64>) -> Self {
        self.global_limiter = Arc::new(RateLimiter::new(bytes_per_sec));
        self
    }

    /// Update the shared global speed cap without rebuilding the client.
    pub fn set_global_limit(&mut self, bytes_per_sec: Option<u64>) {
        self.global_limiter = Arc::new(RateLimiter::new(bytes_per_sec));
    }
    pub fn with_host_stats(mut self, stats: Arc<HostStats>) -> Self {
        self.host_stats = stats;
        self
    }
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
    pub fn config(&self) -> &DownloadConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: DownloadConfig) {
        self.config = config;
    }

    /// Run a download to completion (or cancellation/failure).
    pub async fn download(
        &self,
        req: DownloadRequest,
        events: Option<EventSender>,
        control: Control,
        refresher: Option<RefreshFn>,
    ) -> Result<DownloadOutcome> {
        if req.urls.is_empty() {
            return Err(Error::NoUrl);
        }

        // Probe mirrors; the first that answers provides the reference metadata.
        let mut meta: Option<Meta> = None;
        'mirrors: for u in &req.urls {
            for attempt in 0..=self.config.max_retries {
                if attempt > 0 {
                    tokio::time::sleep(backoff_delay(attempt)).await;
                }
                if let Ok(m) = probe(&self.client, u, self.config.request_timeout).await {
                    meta = Some(m);
                    break 'mirrors;
                }
            }
        }
        let meta = meta.ok_or(Error::AllMirrorsFailed)?;
        let host = url::Url::parse(&meta.final_url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_default();

        // Resolve output path and ensure its directory exists.
        let output = req.output.clone();
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let state_path = DownloadState::state_path(&output);

        let max_conn = req.connections.clamp(1, crate::MAX_CONNECTIONS);
        let supports = meta.supports_ranges && meta.size.map(|s| s > 0).unwrap_or(false);

        if !supports {
            return self
                .download_single(&req, &output, &state_path, events, control)
                .await;
        }

        let total = meta.size.unwrap();

        // Starting connection count: remembered best for the host, clamped by cap.
        let host_cap = self.host_stats.host_cap(&host);
        let initial_conn = if req.autotune {
            // Start from the host's remembered best (if recent) or a sensible
            // default; the tuner then measures up (and down) from there.
            self.host_stats
                .best_connections(&host)
                .unwrap_or(crate::tuner::DEFAULT_START_CONNECTIONS)
                .clamp(1, host_cap.unwrap_or(max_conn).clamp(1, max_conn))
        } else {
            max_conn
        };
        // Allow enough live segments for the most connections this download may use.
        let steal_cap = max_conn.saturating_mul(4).max(8);

        // Load resume state if validators still match.
        let mut resumed = false;
        let table = match DownloadState::load_for_output(&output)? {
            Some(st)
                if !st.segments.is_empty()
                    && st.validators_match(
                        Some(total),
                        meta.etag.as_deref(),
                        meta.last_modified.as_deref(),
                    ) =>
            {
                resumed = true;
                SegmentTable::from_state(
                    &st.segments,
                    self.config.min_segment_size,
                    steal_cap,
                )
            }
            other => {
                if other.is_some() {
                    // Validators changed: start over.
                    DownloadState::remove(&state_path)?;
                }
                let ranges = split_initial(total, initial_conn);
                SegmentTable::from_ranges(
                    &ranges,
                    self.config.min_segment_size,
                    steal_cap,
                )
            }
        };

        // Preallocate the output file to its full size.
        preallocate(&output, total)?;

        let already = table.snapshot().iter().map(|s| s.downloaded).sum::<u64>();

        let global_fail_bound = if self.config.max_global_failures == 0 {
            self.config
                .max_retries
                .saturating_mul(initial_conn)
                .max(self.config.max_retries * 2)
        } else {
            self.config.max_global_failures
        };

        let fatal_slot: Arc<Mutex<Option<Error>>> = Arc::new(Mutex::new(None));
        let write_pending = Arc::new(AtomicUsize::new(0));
        let writer = spawn_disk_writer(output.clone(), fatal_slot.clone(), write_pending.clone());

        let ctx = Arc::new(Ctx {
            writer: Mutex::new(Some(writer)),
            pool: MirrorPool::new(&req.urls, events.clone()),
            path: output.clone(),
            total,
            table,
            downloaded: AtomicU64::new(already),
            write_pending: write_pending.clone(),
            limiter: RateLimiter::new(req.speed_limit),
            global: self.global_limiter.clone(),
            control: control.clone(),
            events: events.clone(),
            refresher,
            config: self.config.clone(),
            active_workers: AtomicUsize::new(0),
            target: AtomicUsize::new(initial_conn),
            recent_errors: AtomicU64::new(0),
            recent_throttles: AtomicU64::new(0),
            global_fail_streak: AtomicUsize::new(0),
            global_fail_bound,
            fatal: fatal_slot,
            notify: Notify::new(),
            final_connections: AtomicUsize::new(initial_conn),
            busy_workers: AtomicUsize::new(0),
            range_fallback: AtomicBool::new(false),
            last_progress_emit: Mutex::new(Instant::now()),
            trace: Mutex::new(Trace::new()),
        });

        ctx.emit(ProgressEvent::Started {
            total_size: Some(total),
            connections: initial_conn,
            resumed_from: already,
            supports_ranges: true,
        });

        let start = Instant::now();

        // Persist initial state so a crash before the first checkpoint resumes.
        persist_state(&ctx, &req, &meta, &state_path, initial_conn);

        // Spawn the initial workers.
        for _ in 0..initial_conn {
            spawn_worker(ctx.clone());
        }

        // Supervisor: samples throughput, auto-tunes, and checkpoints state.
        let supervisor = tokio::spawn(supervise(
            ctx.clone(),
            req.clone(),
            meta.clone(),
            state_path.clone(),
            host.clone(),
            self.host_stats.clone(),
            max_conn,
            initial_conn,
            host_cap,
            start,
        ));

        // Wait until every byte is downloaded or the job stops.
        loop {
            if ctx.table.all_done() {
                break;
            }
            if ctx.range_fallback.load(Ordering::Acquire) {
                break;
            }
            if ctx.fatal.lock().unwrap().is_some() || ctx.control.is_cancelled() {
                break;
            }
            if ctx.active_workers.load(Ordering::Acquire) == 0 {
                reconcile_workers(&ctx);
            }
            let _ = tokio::time::timeout(Duration::from_millis(150), ctx.notify.notified()).await;
        }
        supervisor.abort();

        if control.is_cancelled() {
            *ctx.writer.lock().unwrap() = None;
            ctx.notify.notify_waiters();
            for _ in 0..200 {
                if ctx.active_workers.load(Ordering::Acquire) == 0 {
                    break;
                }
                ctx.notify.notify_waiters();
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }

        if ctx.range_fallback.load(Ordering::Acquire) && !control.is_cancelled() {
            ctx.target.store(0, Ordering::Release);
            ctx.notify.notify_waiters();
            for _ in 0..500 {
                if ctx.active_workers.load(Ordering::Acquire) == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let _ = std::fs::remove_file(&output);
            let _ = DownloadState::remove(&state_path);
            return self
                .download_single(&req, &output, &state_path, events, control)
                .await;
        }

        // Surface any fatal error or cancellation.
        if let Some(err) = ctx.fatal.lock().unwrap().take() {
            persist_state(&ctx, &req, &meta, &state_path, initial_conn);
            return Err(err);
        }
        if control.is_cancelled() {
            let _ = std::fs::remove_file(&output);
            let _ = DownloadState::remove(&state_path);
            ctx.emit(ProgressEvent::Failed {
                error: "cancelled".into(),
            });
            return Err(Error::Cancelled);
        }
        if !ctx.table.all_done() {
            persist_state(&ctx, &req, &meta, &state_path, initial_conn);
            return Err(Error::other("download ended with incomplete segments"));
        }

        drain_disk_writes(&ctx).await;

        let elapsed = start.elapsed();
        let connections_used = ctx.final_connections.load(Ordering::Relaxed).max(1);

        // Verify checksums if requested.
        verify_checksums(&output, req.sha256.as_deref(), req.md5.as_deref())?;

        // Success: drop the state sidecar.
        DownloadState::remove(&state_path)?;

        let avg = total as f64 / elapsed.as_secs_f64().max(1e-9);
        ctx.emit(ProgressEvent::Completed {
            total_size: total,
            elapsed_secs: elapsed.as_secs_f64(),
            average_bps: avg,
        });

        Ok(DownloadOutcome {
            output,
            total_size: total,
            elapsed,
            average_bps: avg,
            connections_used,
            resumed,
        })
    }

    /// Fallback for servers without range support (or unknown size): a single
    /// streaming connection written sequentially. Not resumable.
    async fn download_single(
        &self,
        req: &DownloadRequest,
        output: &std::path::Path,
        state_path: &std::path::Path,
        events: Option<EventSender>,
        control: Control,
    ) -> Result<DownloadOutcome> {
        // A fresh single-connection run cannot reuse a segmented state file.
        DownloadState::remove(state_path)?;

        let emit = |ev: ProgressEvent| {
            if let Some(tx) = &events {
                let _ = tx.send(ev);
            }
        };
        emit(ProgressEvent::Started {
            total_size: None,
            connections: 1,
            resumed_from: 0,
            supports_ranges: false,
        });

        let limiter = RateLimiter::new(req.speed_limit);
        let mut ctrl_rx = control.subscribe();
        let start = Instant::now();

        let mut last_err = None;
        for attempt in 0..=self.config.max_retries {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if attempt > 0 {
                let backoff = backoff_delay(attempt);
                emit(ProgressEvent::Retrying {
                    attempt,
                    error: last_err
                        .as_ref()
                        .map(|e: &Error| e.to_string())
                        .unwrap_or_default(),
                });
                interruptible_sleep(&mut ctrl_rx, backoff).await?;
            }

            let url = req.urls[attempt % req.urls.len()].clone();
            match self
                .single_attempt(
                    &url,
                    output,
                    &limiter,
                    &self.global_limiter,
                    &control,
                    &mut ctrl_rx,
                    &events,
                )
                .await
            {
                Ok(total) => {
                    verify_checksums(output, req.sha256.as_deref(), req.md5.as_deref())?;
                    let elapsed = start.elapsed();
                    let avg = total as f64 / elapsed.as_secs_f64().max(1e-9);
                    emit(ProgressEvent::Completed {
                        total_size: total,
                        elapsed_secs: elapsed.as_secs_f64(),
                        average_bps: avg,
                    });
                    return Ok(DownloadOutcome {
                        output: output.to_path_buf(),
                        total_size: total,
                        elapsed,
                        average_bps: avg,
                        connections_used: 1,
                        resumed: false,
                    });
                }
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(e) => last_err = Some(e),
            }
        }
        let err = last_err.unwrap_or(Error::AllMirrorsFailed);
        emit(ProgressEvent::Failed {
            error: err.to_string(),
        });
        Err(err)
    }

    #[allow(clippy::too_many_arguments)]
    async fn single_attempt(
        &self,
        url: &str,
        output: &std::path::Path,
        limiter: &RateLimiter,
        global: &RateLimiter,
        control: &Control,
        ctrl_rx: &mut watch::Receiver<Cmd>,
        events: &Option<EventSender>,
    ) -> Result<u64> {
        use std::io::Write;
        let resp = loop {
            let send_fut = self.client.get(url).send();
            break tokio::select! {
                r = send_fut => r?,
                changed = ctrl_rx.changed() => {
                    if changed.is_err() {
                        return Err(Error::Cancelled);
                    }
                    wait_runnable(ctrl_rx).await?;
                    continue;
                }
            };
        };
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let total_hint = resp.content_length();
        let mut file = std::fs::File::create(output)?;
        let mut stream = resp.bytes_stream();
        let mut written: u64 = 0;
        while let Some(chunk) = loop {
            break tokio::select! {
                item = stream.next() => item,
                changed = ctrl_rx.changed() => {
                    if changed.is_err() {
                        return Err(Error::Cancelled);
                    }
                    wait_runnable(ctrl_rx).await?;
                    continue;
                }
            };
        } {
            wait_runnable(ctrl_rx).await?;
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let chunk = chunk?;
            acquire_rate(limiter, global, chunk.len() as u64, ctrl_rx).await?;
            file.write_all(&chunk)?;
            written += chunk.len() as u64;
            if let Some(tx) = events {
                let _ = tx.send(progress_tick(
                    written,
                    total_hint,
                    0.0,
                    1,
                    None,
                    None,
                    None,
                    Some(1),
                    Some("downloading"),
                    Some(false),
                    None,
                    None,
                    None,
                ));
            }
        }
        file.flush()?;
        Ok(written)
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

fn spawn_worker(ctx: Arc<Ctx>) {
    ctx.active_workers.fetch_add(1, Ordering::AcqRel);
    tokio::spawn(async move {
        let client = match crate::build_segment_client_with(ctx.config.read_timeout) {
            Ok(c) => c,
            Err(e) => {
                ctx.set_fatal(e);
                ctx.active_workers.fetch_sub(1, Ordering::AcqRel);
                ctx.notify.notify_waiters();
                return;
            }
        };
        let mut ctrl_rx = ctx.control.subscribe();
        // Keep claiming work (queue, then stealing) until nothing remains.
        loop {
            if ctx.range_fallback.load(Ordering::Acquire)
                || ctx.fatal.lock().unwrap().is_some()
                || ctx.control.is_cancelled()
            {
                break;
            }
            let target = ctx.target.load(Ordering::Acquire);
            let active = ctx.active_workers.load(Ordering::Acquire);
            if active > target {
                // Over the connection target — retire without taking new work.
                break;
            }
            let target_now = ctx.target.load(Ordering::Acquire);
            let active_now = ctx.active_workers.load(Ordering::Acquire);
            let idle = active_now > ctx.busy_workers.load(Ordering::Acquire);
            let allow_steal = idle && target_now > 1 && active_now > 1;
            let Some(seg) = ctx.table.next_work(allow_steal) else {
                if ctx.table.all_done() {
                    break;
                }
                let _ =
                    tokio::time::timeout(Duration::from_millis(50), ctx.notify.notified()).await;
                continue;
            };
            ctx.busy_workers.fetch_add(1, Ordering::AcqRel);
            seg.set_in_use(true);
            let seg_result = run_segment(&ctx, &client, &seg, &mut ctrl_rx).await;
            seg.set_in_use(false);
            ctx.busy_workers.fetch_sub(1, Ordering::AcqRel);
            match seg_result {
                Ok(()) => {
                    ctx.global_fail_streak.store(0, Ordering::Release);
                }
                Err(Error::Cancelled) => break,
                Err(e @ Error::LinkExpired { .. }) => {
                    ctx.set_fatal(e);
                    break;
                }
                Err(Error::RangeIgnored) => {
                    ctx.range_fallback.store(true, Ordering::Release);
                    ctx.notify.notify_waiters();
                    break;
                }
                Err(e) => {
                    if seg.remaining() == 0 {
                        continue;
                    }
                    ctx.table.requeue(seg);
                    let streak = ctx.global_fail_streak.fetch_add(1, Ordering::AcqRel) + 1;
                    emit_retry(&ctx, streak, &e.to_string());
                    if streak >= ctx.global_fail_bound {
                        ctx.set_fatal(e);
                        break;
                    }
                    ctx.notify.notify_waiters();
                    let _ = interruptible_sleep(&mut ctrl_rx, backoff_delay(streak)).await;
                }
            }
            let target = ctx.target.load(Ordering::Acquire);
            let active = ctx.active_workers.load(Ordering::Acquire);
            if active > target {
                break;
            }
        }
        ctx.active_workers.fetch_sub(1, Ordering::AcqRel);
        ctx.notify.notify_waiters();
    });
}

/// Ensure enough workers exist for the current target while work remains.
fn reconcile_workers(ctx: &Arc<Ctx>) {
    if ctx.table.all_done()
        || ctx.fatal.lock().unwrap().is_some()
        || ctx.control.is_cancelled()
        || ctx.control.is_paused()
    {
        return;
    }
    let target = ctx.target.load(Ordering::Acquire);
    let active = ctx.active_workers.load(Ordering::Acquire);
    if active < target {
        let need = target.saturating_sub(active);
        let remaining = ctx
            .total
            .saturating_sub(ctx.downloaded.load(Ordering::Relaxed));
        let want_queued = target.saturating_sub(1).max(ctx.table.queued());
        while ctx.table.queued() < want_queued {
            if !ctx.table.try_queue_probe_split(remaining) {
                break;
            }
        }
        for _ in 0..need {
            spawn_worker(ctx.clone());
        }
    }
}

async fn flush_segment_buf(
    ctx: &Arc<Ctx>,
    buf: &mut Vec<u8>,
    at: &mut u64,
    seg: &Arc<Segment>,
    ctrl_rx: &mut watch::Receiver<Cmd>,
) -> Result<()> {
    wait_runnable(ctrl_rx).await?;
    if buf.is_empty() {
        return Ok(());
    }
    let len = buf.len();
    acquire_rate(&ctx.limiter, &ctx.global, len as u64, ctrl_rx).await?;
    let offset = *at;
    let data = std::mem::take(buf);
    *at += len as u64;
    seg.current.store(*at, Ordering::Release);
    ctx.downloaded.fetch_add(len as u64, Ordering::Relaxed);
    ctx.write_pending.fetch_add(1, Ordering::AcqRel);
    let tx = ctx.writer.lock().unwrap().clone();
    let send_err = match tx {
        Some(tx) => tx
            .send(WriteChunk {
                offset,
                data,
                seg: seg.clone(),
            })
            .await
            .is_err(),
        None => true,
    };
    if send_err {
        ctx.write_pending.fetch_sub(1, Ordering::AcqRel);
        return Err(Error::other("disk writer stopped"));
    }
    seg.apply_cap_if_due(*at);
    Ok(())
}

async fn handle_read_stall(
    ctx: &Arc<Ctx>,
    seg: &Arc<Segment>,
    ctrl_rx: &mut watch::Receiver<Cmd>,
    write_buf: &mut Vec<u8>,
    write_at: &mut u64,
    attempt: usize,
) -> Result<()> {
    flush_segment_buf(ctx, write_buf, write_at, seg, ctrl_rx).await?;
    ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
    let may_split =
        ctx.target.load(Ordering::Acquire) > 1 && ctx.active_workers.load(Ordering::Acquire) > 1;
    if may_split {
        if let Some(stolen) = ctx.table.split_tail(seg) {
            ctx.table.requeue(stolen);
            ctx.notify.notify_waiters();
        }
    }
    emit_retry(ctx, attempt + 1, "read stall");
    if seg.remaining() > 0 {
        ctx.table.requeue(seg.clone());
    }
    let _ = interruptible_sleep(ctrl_rx, Duration::from_millis(50)).await;
    Ok(())
}

/// Download a single segment (with retries, mirror selection, and link refresh).
async fn run_segment(
    ctx: &Arc<Ctx>,
    client: &reqwest::Client,
    seg: &Arc<Segment>,
    ctrl_rx: &mut watch::Receiver<Cmd>,
) -> Result<()> {
    let mut attempt = 0usize;

    loop {
        wait_runnable(ctrl_rx).await?;

        let begin = seg.current();
        let end = seg.download_end();
        if begin > end {
            seg.mark_done();
            seg.clear_cap_end();
            return Ok(());
        }

        let mirror = ctx.pool.pick();
        let url = ctx.pool.url_of(&mirror);
        let range = format!("bytes={}-{}", begin, end);

        let req_builder = client.get(&url).header(reqwest::header::RANGE, range);

        let resp = tokio::select! {
            res = req_builder.send() => res,
            changed = ctrl_rx.changed() => {
                if changed.is_err() {
                    return Err(Error::Cancelled);
                }
                wait_runnable(ctrl_rx).await?;
                continue;
            }
        };

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                ctx.pool.demote(&mirror);
                ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
                attempt += 1;
                if attempt > ctx.config.max_retries {
                    return Err(e.into());
                }
                emit_retry(ctx, attempt, &e.to_string());
                interruptible_sleep(ctrl_rx, backoff_delay(attempt)).await?;
                continue;
            }
        };

        let status = resp.status();
        if status == reqwest::StatusCode::OK && (begin > 0 || end < ctx.total.saturating_sub(1)) {
            return Err(Error::RangeIgnored);
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE {
            // Expired link: try to refresh via the hook.
            if let Some(refresher) = &ctx.refresher {
                if let Some(fresh) = refresher(url.clone()).await {
                    ctx.pool.refresh(&mirror, fresh);
                    continue;
                }
            }
            return Err(Error::LinkExpired {
                status: status.as_u16(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        {
            ctx.pool.demote(&mirror);
            ctx.recent_throttles.fetch_add(1, Ordering::Relaxed);
            let delay = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| parse_retry_after(v, RETRY_AFTER_CAP))
                .unwrap_or_else(|| backoff_delay(attempt + 1));
            emit_retry(
                ctx,
                attempt + 1,
                &format!("HTTP {} (throttled)", status.as_u16()),
            );
            interruptible_sleep(ctrl_rx, delay).await?;
            let active = ctx.active_workers.load(Ordering::Acquire);
            let target = ctx.target.load(Ordering::Acquire);
            if active > target && seg.remaining() > 0 {
                ctx.table.requeue(seg.clone());
                return Ok(());
            }
            continue;
        }
        if status.is_server_error() {
            ctx.pool.demote(&mirror);
            ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(Error::Status(status.as_u16()));
            }
            emit_retry(ctx, attempt, &format!("HTTP {}", status.as_u16()));
            interruptible_sleep(ctrl_rx, backoff_delay(attempt)).await?;
            continue;
        }
        if !(status == reqwest::StatusCode::PARTIAL_CONTENT || status.is_success()) {
            ctx.pool.demote(&mirror);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(Error::Status(status.as_u16()));
            }
            emit_retry(ctx, attempt, &format!("HTTP {}", status.as_u16()));
            interruptible_sleep(ctrl_rx, backoff_delay(attempt)).await?;
            continue;
        }

        // Stream the body, batched into positioned writes.
        let mut pos = begin;
        let mut stream = resp.bytes_stream();
        let mut stream_err: Option<Error> = None;
        let mut write_buf = Vec::with_capacity(SEGMENT_WRITE_BUF);
        let mut write_at = pos;
        let mut stall_deadline = tokio::time::Instant::now() + SEGMENT_STALL_TIMEOUT;
        let mut stalled = false;

        loop {
            let chunk_res = tokio::select! {
                _ = tokio::time::sleep_until(stall_deadline) => {
                    handle_read_stall(ctx, seg, ctrl_rx, &mut write_buf, &mut write_at, attempt)
                    .await?;
                    stalled = true;
                    break;
                }
                item = stream.next() => item,
                changed = ctrl_rx.changed() => {
                    if changed.is_err() {
                        return Err(Error::Cancelled);
                    }
                    wait_runnable(ctrl_rx).await?;
                    continue;
                }
            };
            match chunk_res {
                Some(Ok(chunk)) => {
                    if matches!(*ctrl_rx.borrow(), Cmd::Paused) {
                        wait_runnable(ctrl_rx).await?;
                    }
                    if chunk.is_empty() {
                        continue;
                    }
                    stall_deadline = tokio::time::Instant::now() + SEGMENT_STALL_TIMEOUT;
                    let cur_end = seg.download_end();
                    let mut bytes: &[u8] = &chunk;
                    let last = pos + bytes.len() as u64 - 1;
                    let truncated = if last > cur_end {
                        let keep = (cur_end + 1).saturating_sub(pos) as usize;
                        bytes = &bytes[..keep.min(bytes.len())];
                        true
                    } else {
                        false
                    };

                    if !bytes.is_empty() {
                        if write_buf.is_empty() {
                            write_at = pos;
                        }
                        write_buf.extend_from_slice(bytes);
                        pos += bytes.len() as u64;
                        if write_buf.len() >= SEGMENT_WRITE_BUF {
                            flush_segment_buf(ctx, &mut write_buf, &mut write_at, seg, ctrl_rx)
                                .await?;
                        }
                    }

                    if truncated || pos > cur_end {
                        flush_segment_buf(ctx, &mut write_buf, &mut write_at, seg, ctrl_rx).await?;
                        break;
                    }
                }
                Some(Err(e)) => {
                    flush_segment_buf(ctx, &mut write_buf, &mut write_at, seg, ctrl_rx).await?;
                    ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
                    stream_err = Some(e.into());
                    break;
                }
                None => {
                    flush_segment_buf(ctx, &mut write_buf, &mut write_at, seg, ctrl_rx).await?;
                    break;
                }
            }
        }

        if stalled {
            continue;
        }

        if let Some(e) = stream_err {
            // Connection dropped mid-segment: retry, resuming from `pos`.
            ctx.pool.demote(&mirror);
            ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(e);
            }
            emit_retry(ctx, attempt, &e.to_string());
            interruptible_sleep(ctrl_rx, backoff_delay(attempt)).await?;
            continue;
        }

        if pos > seg.end() || seg.remaining() == 0 {
            seg.mark_done();
            ctx.pool.reward(&mirror);
            return Ok(());
        }
        // Otherwise loop to continue the remaining bytes (e.g. short response).
        attempt = 0;
    }
}

fn emit_retry(ctx: &Ctx, attempt: usize, error: &str) {
    ctx.emit(ProgressEvent::Retrying {
        attempt,
        error: error.to_string(),
    });
}

// ---------------------------------------------------------------------------
// Supervisor: sampling, auto-tuning, state checkpoints
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn supervise(
    ctx: Arc<Ctx>,
    req: DownloadRequest,
    meta: Meta,
    state_path: PathBuf,
    host: String,
    host_stats: Arc<HostStats>,
    max_conn: usize,
    initial_conn: usize,
    host_cap: Option<usize>,
    start: Instant,
) {
    let mut last_bytes = ctx.downloaded.load(Ordering::Relaxed);
    let mut last_time = Instant::now();
    let mut speed_ewma = 0.0f64;
    let speed_alpha = 0.35;

    let cap = host_cap.unwrap_or(max_conn).clamp(1, max_conn);
    let mut tuner = if req.autotune {
        Some(Tuner::new(initial_conn, max_conn, Some(cap)))
    } else {
        None
    };
    let mut target = initial_conn;
    ctx.target.store(target, Ordering::Release);
    ctx.final_connections
        .fetch_max(target, Ordering::Release);

    let mut ctrl_rx = ctx.control.subscribe();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(ctx.config.tune_interval) => {}
            res = ctrl_rx.changed() => {
                if res.is_err() || ctx.control.is_cancelled() {
                    break;
                }
                continue;
            }
        }

        let now = Instant::now();
        let bytes = ctx.downloaded.load(Ordering::Relaxed);
        let dt = now
            .saturating_duration_since(last_time)
            .as_secs_f64()
            .max(1e-6);
        let speed = (bytes.saturating_sub(last_bytes)) as f64 / dt;
        last_bytes = bytes;
        last_time = now;

        if speed_ewma <= 0.0 {
            speed_ewma = speed;
        } else {
            speed_ewma = speed_alpha * speed + (1.0 - speed_alpha) * speed_ewma;
        }

        let active = ctx.active_workers.load(Ordering::Acquire);
        let elapsed = start.elapsed().as_secs_f64();
        let avg = bytes as f64 / elapsed.max(1e-6);
        let eta = if speed_ewma > 1.0 {
            Some(ctx.total.saturating_sub(bytes) as f64 / speed_ewma)
        } else {
            None
        };

        let status = if ctx.control.is_paused() {
            "paused"
        } else {
            "downloading"
        };

        let should_emit = {
            let mut last = ctx.last_progress_emit.lock().unwrap();
            if last.elapsed() >= PROGRESS_EMIT_INTERVAL {
                *last = now;
                true
            } else {
                false
            }
        };
        if should_emit {
            ctx.emit(progress_tick(
                bytes,
                Some(ctx.total),
                speed_ewma,
                target,
                eta,
                Some(avg),
                Some(elapsed),
                Some(active),
                Some(status),
                Some(true),
                if host.is_empty() { None } else { Some(&host) },
                Some("HTTP/1.1"),
                Some(ctx.table.snapshot()),
            ));
        }

        persist_state(&ctx, &req, &meta, &state_path, target);

        let errors = ctx.recent_errors.swap(0, Ordering::Relaxed);
        let throttles = ctx.recent_throttles.swap(0, Ordering::Relaxed);

        let busy = ctx.busy_workers.load(Ordering::Relaxed);
        let queued = ctx.table.queued();
        ctx.trace.lock().unwrap().maybe_tick(
            active,
            busy,
            target,
            queued,
            errors,
            &ctx.table.segments(),
            bytes,
        );

        if let Some(t) = tuner.as_mut() {
            let remaining = ctx.total.saturating_sub(bytes);
            let out = t.tick(&TunerInput {
                throughput: speed,
                active_connections: active.max(1),
                throttle_events: throttles,
                error_events: errors,
                bytes_remaining: remaining,
            });
            if out.target != target {
                target = out.target;
                ctx.target.store(target, Ordering::Release);
                ctx.final_connections
                    .fetch_max(target, Ordering::Release);
                ctx.emit(ProgressEvent::ConnectionsChanged {
                    count: target,
                    reason: if throttles > 0 {
                        "throttle backoff".into()
                    } else if errors > 0 {
                        format!("backing off after {errors} error(s)")
                    } else {
                        "auto-tune".into()
                    },
                });
            }
            if throttles > 0 && !host.is_empty() {
                host_stats.record_throttle(&host, t.host_cap());
            }
        }

        reconcile_workers(&ctx);

        if ctx.active_workers.load(Ordering::Acquire) == 0
            && !ctx.table.all_done()
            && !ctx.control.is_cancelled()
            && ctx.fatal.lock().unwrap().is_none()
        {
            reconcile_workers(&ctx);
        }

        if (ctx.active_workers.load(Ordering::Acquire) == 0 && ctx.table.all_done())
            || ctx.control.is_cancelled()
            || ctx.fatal.lock().unwrap().is_some()
        {
            break;
        }
    }

    if req.autotune {
        if let Some(t) = tuner.as_ref() {
            if t.best_throughput() > 0.0 && !host.is_empty() {
                host_stats.observe(&host, t.best_connections(), t.best_throughput());
            }
            ctx.final_connections
                .fetch_max(t.target().max(1), Ordering::Release);
        }
    }
}

fn persist_state(
    ctx: &Ctx,
    req: &DownloadRequest,
    meta: &Meta,
    state_path: &std::path::Path,
    connections: usize,
) {
    let st = DownloadState {
        version: crate::state::STATE_VERSION,
        urls: req.urls.clone(),
        output: ctx.path.clone(),
        total_size: Some(ctx.total),
        etag: meta.etag.clone(),
        last_modified: meta.last_modified.clone(),
        supports_ranges: true,
        connections,
        segments: ctx.table.snapshot(),
    };
    let _ = st.save(state_path);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Preallocate (or keep) the output file at the given size.
pub fn preallocate(path: &std::path::Path, size: u64) -> Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(path)?;
    if file.metadata()?.len() != size {
        file.set_len(size)?;
    }
    Ok(())
}

/// Exponential backoff with full jitter.
fn backoff_delay(attempt: usize) -> Duration {
    use rand::Rng;
    let base = 200u64; // ms
    let cap = 30_000u64;
    let exp = base.saturating_mul(1u64 << attempt.min(7)).min(cap);
    let jitter = rand::thread_rng().gen_range(0..=exp.max(1));
    Duration::from_millis(exp / 2 + jitter / 2)
}

fn verify_checksums(path: &std::path::Path, sha256: Option<&str>, md5: Option<&str>) -> Result<()> {
    if let Some(expected) = sha256 {
        checksum::verify_file(path, Algo::Sha256, expected)?;
    }
    if let Some(expected) = md5 {
        checksum::verify_file(path, Algo::Md5, expected)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_bounded() {
        for attempt in 1..=10 {
            let d = backoff_delay(attempt);
            assert!(d <= Duration::from_secs(30));
        }
    }

    #[test]
    fn disposition_parsing() {
        assert_eq!(
            filename_from_disposition("attachment; filename=\"my file.zip\""),
            Some("my file.zip".to_string())
        );
        assert_eq!(
            filename_from_disposition("attachment; filename*=UTF-8''my%20file.zip"),
            Some("my file.zip".to_string())
        );
        // Path traversal is stripped.
        assert_eq!(
            filename_from_disposition("attachment; filename=\"../../etc/passwd\""),
            Some("passwd".to_string())
        );
    }

    #[test]
    fn preallocate_sets_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.bin");
        preallocate(&p, 4096).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 4096);
        // Idempotent.
        preallocate(&p, 4096).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 4096);
    }
}
