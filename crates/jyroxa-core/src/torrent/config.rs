//! Speed-oriented defaults for the BitTorrent session.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TorrentTuning {
    /// Max concurrently connected peers per torrent.
    pub peer_limit: usize,
    /// Merge [`super::trackers::builtin_trackers`] into every torrent.
    pub merge_builtin_trackers: bool,
    /// Persist session state for resume across restarts.
    pub fastresume: bool,
    /// Stop seeding when uploaded/downloaded ratio reaches this (0 = disabled).
    pub ratio_limit: f64,
    /// Allow upload/seeding after download completes.
    pub allow_seeding: bool,
    /// Optional download cap in bytes/s (0 = unlimited).
    pub download_limit_bps: u64,
    /// Optional upload cap in bytes/s (0 = unlimited).
    pub upload_limit_bps: u64,
    /// Folder for librqbit session persistence JSON.
    pub session_data_dir: Option<PathBuf>,
}

impl Default for TorrentTuning {
    fn default() -> Self {
        Self {
            peer_limit: 200,
            merge_builtin_trackers: true,
            fastresume: true,
            ratio_limit: 1.0,
            allow_seeding: false,
            download_limit_bps: 0,
            upload_limit_bps: 0,
            session_data_dir: None,
        }
    }
}

impl TorrentTuning {
    pub fn with_output_root(mut self, root: PathBuf) -> Self {
        self.session_data_dir = Some(root.join(".jyroxa-torrent-session"));
        self
    }
}