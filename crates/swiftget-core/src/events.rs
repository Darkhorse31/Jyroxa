//! Progress events emitted by the engine for UIs to consume.

use serde::{Deserialize, Serialize};

/// Sender half used by the engine. UIs own the matching receiver.
pub type EventSender = tokio::sync::mpsc::UnboundedSender<ProgressEvent>;
/// Receiver half handed to UIs.
pub type EventReceiver = tokio::sync::mpsc::UnboundedReceiver<ProgressEvent>;

/// Create an unbounded event channel for a download.
pub fn channel() -> (EventSender, EventReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}

/// A single progress event. Serializable so it can cross the FFI boundary as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// Emitted once metadata is known and the download begins.
    Started {
        total_size: Option<u64>,
        connections: usize,
        resumed_from: u64,
        supports_ranges: bool,
    },
    /// Aggregate progress with a smoothed speed sample.
    Progress {
        downloaded: u64,
        total: Option<u64>,
        speed_bps: f64,
        connections: usize,
    },
    /// Per-segment progress, useful for segmented progress bars.
    SegmentProgress {
        index: usize,
        start: u64,
        current: u64,
        end: u64,
    },
    /// The auto-tuner changed the number of active connections.
    ConnectionsChanged { count: usize, reason: String },
    /// A mirror was demoted after repeated failures.
    MirrorDemoted { url: String },
    /// A retry is about to happen after a transient error.
    Retrying { attempt: usize, error: String },
    /// An expired link was refreshed via the refresh hook.
    LinkRefreshed { old: String, new: String },
    /// The download finished successfully.
    Completed {
        total_size: u64,
        elapsed_secs: f64,
        average_bps: f64,
    },
    /// The download failed terminally.
    Failed { error: String },
}
