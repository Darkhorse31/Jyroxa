//! BitTorrent engine (librqbit) with magnet support and tracker boosting.

mod config;
#[cfg(feature = "torrent-engine")]
mod engine;
mod magnet;
mod trackers;

pub use config::TorrentTuning;
#[cfg(feature = "torrent-engine")]
pub use engine::{
    TorrentHandle, TorrentJobOptions, TorrentSession, TorrentSnapshot, TorrentSource,
};
pub use magnet::{
    is_magnet_uri, is_torrent_source, magnet_display_name, magnet_trackers, magnet_with_trackers,
    parse_info_hash_hex,
};
pub use trackers::{builtin_trackers, merge_tracker_lists, BUILTIN_TRACKERS};