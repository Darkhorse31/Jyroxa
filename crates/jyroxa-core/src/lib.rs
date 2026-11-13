//! Jyroxa core download engine.
//!
//! This crate implements a real, multi-connection segmented HTTP(S) download
//! engine with dynamic work-stealing, per-host connection auto-tuning,
//! multi-mirror racing, resumable state, a token-bucket speed limiter, a
//! queue/scheduler and a JDownloader-style LinkGrabber.
//!
//! The engine is transport-agnostic above `reqwest` and emits [`ProgressEvent`]s
//! so that UIs (CLI, Tauri desktop, Android via FFI) can render progress.

pub mod checksum;
pub mod download;
pub mod error;
pub mod events;
pub mod hoststats;
pub mod limiter;
pub mod linkgrabber;
pub mod naming;
pub mod probe;
pub mod queue;
pub mod segment;
pub mod state;
pub mod trace;
pub mod tuner;

#[cfg(feature = "telegram")]
pub mod telegram;

pub use download::{
    Control, DownloadConfig, DownloadOutcome, DownloadRequest, Downloader, RefreshFn,
};
pub use error::{Error, Result};
pub use events::ProgressEvent;
pub use hoststats::HostStats;
pub use limiter::RateLimiter;
pub use linkgrabber::{GrabOptions, LinkGrabber, LinkInfo, Package};
pub use naming::{category_for, dedupe_path, suggest_filename, Category};
pub use probe::{probe_url, ProbeInfo};
pub use queue::{QueueConfig, Scheduler, TaskId, TaskPriority, TaskSpec};
pub use state::{DownloadState, SegmentState};
pub use tuner::{parse_retry_after, Tuner, TunerInput, TunerOutput};

#[cfg(feature = "telegram")]
pub use telegram::{
    extract_telegram_urls, is_telegram_post_url, parse_telegram_link, probe_post, resolve_post,
    TelegramAccountView, TelegramClientPool, TelegramError, TelegramLoginStep, TelegramMediaItem,
    TelegramPostRef,
};

/// Hard upper bound on the number of simultaneous connections for a single
/// download, matching the capability advertised to users.
pub const MAX_CONNECTIONS: usize = 32;

/// Whether this build was compiled with the optional HTTP/3 (QUIC) feature.
///
/// HTTP/3 is an opt-in feature flag (`--features http3`) and is off by default.
/// When disabled the engine transparently uses HTTP/2.
pub const HTTP3_ENABLED: bool = cfg!(feature = "http3");

/// A convenient default `User-Agent`.
pub const USER_AGENT: &str = concat!("Jyroxa/", env!("CARGO_PKG_VERSION"));

/// Default idle/read timeout for open HTTP responses (no bytes arriving).
pub const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Build a tuned [`reqwest::Client`] suitable for the engine and LinkGrabber.
pub fn build_client() -> Result<reqwest::Client> {
    build_client_with(DEFAULT_READ_TIMEOUT)
}

/// Build a client with a custom per-read idle timeout (no total request deadline).
pub fn build_client_with(read_timeout: std::time::Duration) -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .pool_max_idle_per_host(MAX_CONNECTIONS)
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(20))
        .read_timeout(read_timeout)
        .build()?;
    Ok(client)
}

/// Build a [`reqwest::Client`] for segment workers that forces HTTP/1.1.
///
/// HTTP/2 multiplexes all streams over a single TCP connection, so N "connections"
/// are really N streams contending for the same pipe. By forcing HTTP/1.1, each
/// segment worker opens its own TCP connection — like IDM — giving real
/// parallelism across connections.
pub fn build_segment_client() -> Result<reqwest::Client> {
    build_segment_client_with(DEFAULT_READ_TIMEOUT)
}

/// Segment worker client with HTTP/1.1 and an idle read timeout.
pub fn build_segment_client_with(read_timeout: std::time::Duration) -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .http1_only()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(10))
        .tcp_nodelay(true)
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(20))
        .read_timeout(read_timeout)
        .build()?;
    Ok(client)
}
