//! The multi-connection, multi-mirror, auto-tuning download engine.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::StreamExt;
use tokio::sync::{watch, Notify};

use crate::checksum::{self, Algo};
use crate::error::{Error, Result};
use crate::events::{EventSender, ProgressEvent};
use crate::hoststats::HostStats;
use crate::limiter::RateLimiter;
use crate::segment::{split_initial, Segment, SegmentTable};
use crate::state::DownloadState;

/// A callback that, given an expired URL, asynchronously returns a fresh one.
/// Used for links that return 403/410 after their signed-token expires.
pub type RefreshFn =
    Arc<dyn Fn(String) -> BoxFuture<'static, Option<String>> + Send + Sync>;

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
    /// Enable per-host connection auto-tuning.
    pub autotune: bool,
    /// How often to sample throughput / tune / persist state.
    pub tune_interval: Duration,
    /// Request timeout per segment connection.
    pub request_timeout: Duration,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        DownloadConfig {
            max_connections: crate::MAX_CONNECTIONS,
            initial_connections: 4,
            min_segment_size: 1 << 20, // 1 MiB
            max_retries: 5,
            autotune: true,
            tune_interval: Duration::from_millis(1000),
            request_timeout: Duration::from_secs(60),
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
    h.get(name).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

/// Parse `Content-Disposition` for a filename.
pub fn filename_from_disposition(value: &str) -> Option<String> {
    // Prefer RFC 5987 filename*=
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            let rest = rest.trim();
            // e.g. UTF-8''my%20file.zip
            if let Some(idx) = rest.rfind("''") {
                let enc = &rest[idx + 2..];
                if let Ok(dec) = percent_decode(enc) {
                    return Some(sanitize_name(&dec));
                }
            }
        }
    }
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(sanitize_name(v));
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> std::result::Result<String, ()> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16).ok_or(())?;
                let lo = (bytes[i + 2] as char).to_digit(16).ok_or(())?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

/// Strip path separators / parent references from a candidate filename.
pub fn sanitize_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches('.');
    let cleaned: String = base
        .chars()
        .map(|c| if "<>:\"|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

/// Derive a filename from a URL path, if any.
pub fn filename_from_url(u: &url::Url) -> Option<String> {
    let seg = u.path_segments()?.last()?;
    if seg.is_empty() {
        None
    } else {
        Some(sanitize_name(seg))
    }
}

async fn probe(client: &reqwest::Client, url: &str, timeout: Duration) -> Result<Meta> {
    use reqwest::header as h;
    let mut meta = Meta {
        final_url: url.to_string(),
        ..Default::default()
    };

    // 1. HEAD for size / validators / filename.
    if let Ok(resp) = client.head(url).timeout(timeout).send().await {
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
                if let Some(total) = cr.rsplit('/').next().and_then(|s| s.trim().parse::<u64>().ok()) {
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
        let min = self
            .mirrors
            .iter()
            .map(|m| m.penalty.load(Ordering::Relaxed))
            .min()
            .unwrap_or(0);
        for _ in 0..n {
            let i = self.rr.fetch_add(1, Ordering::Relaxed) % n;
            if self.mirrors[i].penalty.load(Ordering::Relaxed) <= min + 1 {
                return self.mirrors[i].clone();
            }
        }
        let i = self.rr.fetch_add(1, Ordering::Relaxed) % n;
        self.mirrors[i].clone()
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

struct Ctx {
    client: reqwest::Client,
    pool: MirrorPool,
    path: PathBuf,
    total: u64,
    table: SegmentTable,
    downloaded: AtomicU64,
    limiter: RateLimiter,
    global: Arc<RateLimiter>,
    control: Control,
    events: Option<EventSender>,
    refresher: Option<RefreshFn>,
    config: DownloadConfig,
    active_workers: AtomicUsize,
    recent_errors: AtomicU64,
    fatal: Mutex<Option<Error>>,
    notify: Notify,
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

    /// Open a dedicated write handle for a worker (avoids a shared file cursor).
    fn open_write(&self) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new().write(true).open(&self.path)
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// The download engine. Cheap to clone-share via the owned client.
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
            client: crate::build_client()?,
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
        for u in &req.urls {
            match probe(&self.client, u, self.config.request_timeout).await {
                Ok(m) => {
                    meta = Some(m);
                    break;
                }
                Err(_) => continue,
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

        // Starting connection count: remembered best for the host, clamped.
        let initial_conn = if req.autotune {
            self.host_stats
                .best_connections(&host)
                .unwrap_or(self.config.initial_connections)
                .clamp(1, max_conn)
        } else {
            max_conn
        };

        // Load resume state if validators still match.
        let mut resumed = false;
        let table = match DownloadState::load(&state_path)? {
            Some(st)
                if !st.segments.is_empty()
                    && st.validators_match(
                        Some(total),
                        meta.etag.as_deref(),
                        meta.last_modified.as_deref(),
                    ) =>
            {
                resumed = true;
                SegmentTable::from_state(&st.segments, self.config.min_segment_size)
            }
            other => {
                if other.is_some() {
                    // Validators changed: start over.
                    DownloadState::remove(&state_path)?;
                }
                let ranges = split_initial(total, initial_conn);
                SegmentTable::from_ranges(&ranges, self.config.min_segment_size)
            }
        };

        // Preallocate the output file to its full size.
        preallocate(&output, total)?;

        let already = table.snapshot().iter().map(|s| s.downloaded).sum::<u64>();

        let ctx = Arc::new(Ctx {
            client: self.client.clone(),
            pool: MirrorPool::new(&req.urls, events.clone()),
            path: output.clone(),
            total,
            table,
            downloaded: AtomicU64::new(already),
            limiter: RateLimiter::new(req.speed_limit),
            global: self.global_limiter.clone(),
            control: control.clone(),
            events: events.clone(),
            refresher,
            config: self.config.clone(),
            active_workers: AtomicUsize::new(0),
            recent_errors: AtomicU64::new(0),
            fatal: Mutex::new(None),
            notify: Notify::new(),
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
            start,
        ));

        // Wait for all workers to finish.
        loop {
            if ctx.active_workers.load(Ordering::Acquire) == 0 {
                break;
            }
            let _ = tokio::time::timeout(Duration::from_millis(150), ctx.notify.notified()).await;
        }
        supervisor.abort();

        // Surface any fatal error or cancellation.
        if let Some(err) = ctx.fatal.lock().unwrap().take() {
            persist_state(&ctx, &req, &meta, &state_path, initial_conn);
            return Err(err);
        }
        if control.is_cancelled() {
            persist_state(&ctx, &req, &meta, &state_path, initial_conn);
            return Err(Error::Cancelled);
        }
        if !ctx.table.all_done() {
            persist_state(&ctx, &req, &meta, &state_path, initial_conn);
            return Err(Error::other("download ended with incomplete segments"));
        }

        let elapsed = start.elapsed();
        let connections_used = ctx.table.len();

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
                tokio::time::sleep(backoff).await;
            }

            let url = req.urls[attempt % req.urls.len()].clone();
            match self
                .single_attempt(&url, output, &limiter, &self.global_limiter, &control, &mut ctrl_rx, &events)
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
        let resp = self
            .client
            .get(url)
            .timeout(self.config.request_timeout)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let total_hint = resp.content_length();
        let mut file = std::fs::File::create(output)?;
        let mut stream = resp.bytes_stream();
        let mut written: u64 = 0;
        while let Some(chunk) = stream.next().await {
            wait_runnable(ctrl_rx).await?;
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let chunk = chunk?;
            limiter.acquire(chunk.len() as u64).await;
            global.acquire(chunk.len() as u64).await;
            file.write_all(&chunk)?;
            written += chunk.len() as u64;
            if let Some(tx) = events {
                let _ = tx.send(ProgressEvent::Progress {
                    downloaded: written,
                    total: total_hint,
                    speed_bps: 0.0,
                    connections: 1,
                });
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
        let mut ctrl_rx = ctx.control.subscribe();
        // Keep claiming work (queue, then stealing) until nothing remains.
        while let Some(seg) = ctx.table.next_work() {
            if let Err(e) = run_segment(&ctx, &seg, &mut ctrl_rx).await {
                match e {
                    Error::Cancelled => break,
                    other => {
                        ctx.set_fatal(other);
                        break;
                    }
                }
            }
        }
        ctx.active_workers.fetch_sub(1, Ordering::AcqRel);
        ctx.notify.notify_waiters();
    });
}

/// Download a single segment (with retries, mirror selection, and link refresh).
async fn run_segment(
    ctx: &Arc<Ctx>,
    seg: &Arc<Segment>,
    ctrl_rx: &mut watch::Receiver<Cmd>,
) -> Result<()> {
    let mut file = ctx.open_write()?;
    let mut attempt = 0usize;

    loop {
        wait_runnable(ctrl_rx).await?;

        let begin = seg.current();
        let end = seg.end();
        if begin > end {
            seg.mark_done();
            return Ok(());
        }

        let mirror = ctx.pool.pick();
        let url = ctx.pool.url_of(&mirror);
        let range = format!("bytes={}-{}", begin, end);

        let send = ctx
            .client
            .get(&url)
            .header(reqwest::header::RANGE, range)
            .timeout(ctx.config.request_timeout)
            .send()
            .await;

        let resp = match send {
            Ok(r) => r,
            Err(e) => {
                ctx.pool.demote(&mirror);
                ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
                attempt += 1;
                if attempt > ctx.config.max_retries {
                    return Err(e.into());
                }
                emit_retry(ctx, attempt, &e.to_string());
                tokio::time::sleep(backoff_delay(attempt)).await;
                continue;
            }
        };

        let status = resp.status();
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
            ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(Error::Status(status.as_u16()));
            }
            emit_retry(ctx, attempt, &format!("HTTP {}", status.as_u16()));
            tokio::time::sleep(backoff_delay(attempt)).await;
            continue;
        }
        if !(status == reqwest::StatusCode::PARTIAL_CONTENT || status.is_success()) {
            ctx.pool.demote(&mirror);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(Error::Status(status.as_u16()));
            }
            emit_retry(ctx, attempt, &format!("HTTP {}", status.as_u16()));
            tokio::time::sleep(backoff_delay(attempt)).await;
            continue;
        }

        // Stream the body, writing at absolute offsets.
        let mut pos = begin;
        let mut stream = resp.bytes_stream();
        let mut stream_err: Option<Error> = None;
        loop {
            // Respect pause/cancel between chunks.
            if let Err(e) = wait_runnable(ctrl_rx).await {
                return Err(e);
            }
            match stream.next().await {
                Some(Ok(chunk)) => {
                    if chunk.is_empty() {
                        continue;
                    }
                    // The segment's end may have shrunk due to a steal.
                    let cur_end = seg.end();
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
                        ctx.limiter.acquire(bytes.len() as u64).await;
                        ctx.global.acquire(bytes.len() as u64).await;
                        pwrite_all(&file, pos, bytes)?;
                        pos += bytes.len() as u64;
                        seg.current.store(pos, Ordering::Release);
                        ctx.downloaded.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                        ctx.emit(ProgressEvent::SegmentProgress {
                            index: seg.index,
                            start: seg.start,
                            current: pos,
                            end: cur_end,
                        });
                    }

                    if truncated || pos > cur_end {
                        break;
                    }
                }
                Some(Err(e)) => {
                    stream_err = Some(e.into());
                    break;
                }
                None => break,
            }
        }

        use std::io::Write;
        let _ = file.flush();

        if let Some(e) = stream_err {
            // Connection dropped mid-segment: retry, resuming from `pos`.
            ctx.pool.demote(&mirror);
            ctx.recent_errors.fetch_add(1, Ordering::Relaxed);
            attempt += 1;
            if attempt > ctx.config.max_retries {
                return Err(e);
            }
            emit_retry(ctx, attempt, &e.to_string());
            tokio::time::sleep(backoff_delay(attempt)).await;
            continue;
        }

        // Did we finish this (possibly shrunk) segment?
        if seg.current() > seg.end() {
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
    start: Instant,
) {
    let mut last_bytes = ctx.downloaded.load(Ordering::Relaxed);
    let mut last_time = Instant::now();
    let mut best_speed = 0.0f64;
    let mut best_conn = initial_conn;
    let mut target = initial_conn;

    loop {
        tokio::time::sleep(ctx.config.tune_interval).await;

        let now = Instant::now();
        let bytes = ctx.downloaded.load(Ordering::Relaxed);
        let dt = now.saturating_duration_since(last_time).as_secs_f64().max(1e-6);
        let speed = (bytes.saturating_sub(last_bytes)) as f64 / dt;
        last_bytes = bytes;
        last_time = now;

        let active = ctx.active_workers.load(Ordering::Acquire);
        ctx.emit(ProgressEvent::Progress {
            downloaded: bytes,
            total: Some(ctx.total),
            speed_bps: speed,
            connections: active,
        });

        // Checkpoint resume state.
        persist_state(&ctx, &req, &meta, &state_path, target);

        if speed > best_speed {
            best_speed = speed;
            best_conn = active.max(1);
        }

        // Auto-tune: ramp up while throughput keeps improving and there were no
        // recent errors; back off (stop adding) otherwise.
        if req.autotune {
            let errors = ctx.recent_errors.swap(0, Ordering::Relaxed);
            let improving = speed >= best_speed * 0.9;
            let remaining = ctx.total.saturating_sub(bytes);
            let worth_it = remaining > ctx.config.min_segment_size * 2;

            if errors == 0 && improving && target < max_conn && active >= target && worth_it {
                target += 1;
                spawn_worker(ctx.clone());
                ctx.emit(ProgressEvent::ConnectionsChanged {
                    count: target,
                    reason: "throughput improving".into(),
                });
            } else if errors > 0 && target > 1 {
                // Back off the target (existing workers finish naturally).
                target = target.saturating_sub(1).max(1);
                ctx.emit(ProgressEvent::ConnectionsChanged {
                    count: target,
                    reason: format!("backing off after {errors} error(s)"),
                });
            }
        }

        if ctx.active_workers.load(Ordering::Acquire) == 0 || ctx.control.is_cancelled() {
            break;
        }
    }

    // Remember the best connection count for this host.
    if req.autotune && best_speed > 0.0 && !host.is_empty() {
        let _ = start; // (start kept for clarity; elapsed computed by caller)
        host_stats.observe(&host, best_conn, best_speed);
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
