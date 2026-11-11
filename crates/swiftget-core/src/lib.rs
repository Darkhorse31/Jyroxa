//! SwiftGet core download engine.
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
pub mod queue;
pub mod segment;
pub mod state;

pub use download::{Control, DownloadConfig, DownloadOutcome, DownloadRequest, Downloader, RefreshFn};
pub use error::{Error, Result};
pub use events::ProgressEvent;
pub use hoststats::HostStats;
pub use limiter::RateLimiter;
pub use linkgrabber::{GrabOptions, LinkGrabber, LinkInfo, Package};
pub use queue::{QueueConfig, Scheduler, TaskId, TaskPriority, TaskSpec};
pub use state::{DownloadState, SegmentState};

/// Hard upper bound on the number of simultaneous connections for a single
/// download, matching the capability advertised to users.
pub const MAX_CONNECTIONS: usize = 32;

/// Whether this build was compiled with the optional HTTP/3 (QUIC) feature.
///
/// HTTP/3 is an opt-in feature flag (`--features http3`) and is off by default.
/// When disabled the engine transparently uses HTTP/2.
pub const HTTP3_ENABLED: bool = cfg!(feature = "http3");

/// A convenient default `User-Agent`.
pub const USER_AGENT: &str = concat!("SwiftGet/", env!("CARGO_PKG_VERSION"));

/// Build a tuned [`reqwest::Client`] suitable for the engine and LinkGrabber.
pub fn build_client() -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .pool_max_idle_per_host(MAX_CONNECTIONS)
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()?;
    Ok(client)
}
