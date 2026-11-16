//! Torrent metadata DTOs (shared with desktop; no librqbit link).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TorrentFileProbe {
    pub index: usize,
    pub path: String,
    pub length: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TorrentProbeInfo {
    pub name: String,
    pub total_bytes: u64,
    pub files: Vec<TorrentFileProbe>,
}