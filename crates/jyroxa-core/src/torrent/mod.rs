//! BitTorrent engine (librqbit) with magnet support and tracker boosting.

mod config;
#[cfg(feature = "torrent-engine")]
mod engine;
mod magnet;
#[cfg(feature = "torrent-engine")]
mod metadata;
mod probe_types;
#[cfg(feature = "torrent-engine")]
mod runner;
mod sidecar_proto;
mod trackers;

pub use config::TorrentTuning;
#[cfg(feature = "torrent-engine")]
pub use engine::{
    TorrentHandle, TorrentJobOptions, TorrentSession, TorrentSnapshot, TorrentSource,
};
pub use magnet::{
    is_magnet_uri, is_torrent_source, magnet_display_name, magnet_trackers, magnet_with_trackers,
    normalize_torrent_input, parse_info_hash_hex,
};
#[cfg(feature = "torrent-engine")]
pub use metadata::probe_torrent_source;
pub use probe_types::{TorrentFileProbe, TorrentProbeInfo};
#[cfg(feature = "torrent-engine")]
pub use runner::{run_until_done_with_stall, wait_initialized_with_progress};
pub use sidecar_proto::{
    parse_error_line, parse_progress_line, stall_user_message, TorrentProgressLine,
    TorrentProgressPhase, METADATA_TIMEOUT_SECS, STALL_TIMEOUT_SECS,
};
pub use trackers::{builtin_trackers, merge_tracker_lists, BUILTIN_TRACKERS};
