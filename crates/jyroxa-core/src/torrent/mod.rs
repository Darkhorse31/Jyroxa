//! BitTorrent engine (librqbit) with magnet support and tracker boosting.

mod config;
#[cfg(feature = "torrent-engine")]
mod engine;
#[cfg(feature = "torrent-engine")]
mod metadata;
#[cfg(feature = "torrent-engine")]
mod runner;
mod sidecar_proto;
mod magnet;
mod probe_types;
mod trackers;

pub use config::TorrentTuning;
#[cfg(feature = "torrent-engine")]
pub use engine::{
    TorrentHandle, TorrentJobOptions, TorrentSession, TorrentSnapshot, TorrentSource,
};
pub use probe_types::{TorrentFileProbe, TorrentProbeInfo};
#[cfg(feature = "torrent-engine")]
pub use metadata::probe_torrent_source;
#[cfg(feature = "torrent-engine")]
pub use runner::{run_until_done_with_stall, wait_initialized_with_progress};
pub use sidecar_proto::{
    parse_error_line, parse_progress_line, stall_user_message, TorrentProgressLine,
    TorrentProgressPhase, METADATA_TIMEOUT_SECS, STALL_TIMEOUT_SECS,
};
pub use magnet::{
    is_magnet_uri, is_torrent_source, magnet_display_name, magnet_trackers, magnet_with_trackers,
    parse_info_hash_hex,
};
pub use trackers::{builtin_trackers, merge_tracker_lists, BUILTIN_TRACKERS};