//! Resume state persisted next to each download as `<output>.jxstate` JSON
//! (legacy `.sgstate` is still read for resume).
//!
//! The state records the server validators (ETag / Last-Modified / size) so a
//! resumed download can detect that the remote file changed, plus the byte
//! ranges and per-segment progress so work can continue exactly where it
//! stopped after a restart or crash.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Current on-disk format version.
pub const STATE_VERSION: u32 = 1;

/// Per-segment persisted progress. `start`..=`end` is inclusive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentState {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    /// Number of bytes of this segment already written.
    pub downloaded: u64,
}

impl SegmentState {
    /// The absolute offset of the next byte to fetch for this segment.
    pub fn cursor(&self) -> u64 {
        self.start + self.downloaded
    }

    /// Whether this segment is fully downloaded.
    pub fn is_complete(&self) -> bool {
        self.downloaded >= self.len()
    }

    /// Total length of the segment in bytes.
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }
}

/// Full persisted download state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadState {
    pub version: u32,
    pub urls: Vec<String>,
    pub output: PathBuf,
    pub total_size: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub supports_ranges: bool,
    pub connections: usize,
    pub segments: Vec<SegmentState>,
}

impl DownloadState {
    /// Compute the sidecar state-file path for an output path.
    pub fn state_path(output: &Path) -> PathBuf {
        let mut s = output.as_os_str().to_os_string();
        s.push(".jxstate");
        PathBuf::from(s)
    }

    /// Legacy sidecar path (pre-rename).
    pub fn legacy_state_path(output: &Path) -> PathBuf {
        let mut s = output.as_os_str().to_os_string();
        s.push(".sgstate");
        PathBuf::from(s)
    }

    /// Load resume state for an output file (`.jxstate`, then `.sgstate`).
    pub fn load_for_output(output: &Path) -> Result<Option<Self>> {
        match Self::load(&Self::state_path(output))? {
            Some(st) => Ok(Some(st)),
            None => Self::load(&Self::legacy_state_path(output)),
        }
    }

    /// Load state from a path, returning `Ok(None)` if the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Atomically persist the state (write to a temp file then rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        let tmp = {
            let mut s = path.as_os_str().to_os_string();
            s.push(".tmp");
            PathBuf::from(s)
        };
        std::fs::write(&tmp, &json)?;
        // Rename is atomic on the same volume; on Windows the destination is
        // replaced if it exists.
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Remove the state file (ignoring "not found").
    pub fn remove(path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Total number of bytes downloaded across all segments.
    pub fn bytes_downloaded(&self) -> u64 {
        self.segments.iter().map(|s| s.downloaded).sum()
    }

    /// Whether every segment is complete.
    pub fn is_complete(&self) -> bool {
        !self.segments.is_empty() && self.segments.iter().all(|s| s.is_complete())
    }

    /// Whether the given fresh server validators are compatible with this state.
    /// A download may only be resumed when nothing material changed.
    pub fn validators_match(
        &self,
        total_size: Option<u64>,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> bool {
        if let (Some(a), Some(b)) = (self.total_size, total_size) {
            if a != b {
                return false;
            }
        }
        if let (Some(a), Some(b)) = (self.etag.as_deref(), etag) {
            if a != b {
                return false;
            }
        }
        if let (Some(a), Some(b)) = (self.last_modified.as_deref(), last_modified) {
            if a != b {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file.bin");
        let sp = DownloadState::state_path(&out);
        assert!(sp.to_string_lossy().ends_with("file.bin.jxstate"));

        let st = DownloadState {
            version: STATE_VERSION,
            urls: vec!["http://example.com/file.bin".into()],
            output: out.clone(),
            total_size: Some(1000),
            etag: Some("\"abc\"".into()),
            last_modified: None,
            supports_ranges: true,
            connections: 4,
            segments: vec![
                SegmentState {
                    index: 0,
                    start: 0,
                    end: 499,
                    downloaded: 100,
                },
                SegmentState {
                    index: 1,
                    start: 500,
                    end: 999,
                    downloaded: 500,
                },
            ],
        };
        assert_eq!(st.bytes_downloaded(), 600);
        assert!(!st.is_complete());

        st.save(&sp).unwrap();
        let loaded = DownloadState::load(&sp).unwrap().unwrap();
        assert_eq!(st, loaded);
    }

    #[test]
    fn load_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join("nope.sgstate");
        assert!(DownloadState::load(&sp).unwrap().is_none());
    }

    #[test]
    fn validator_mismatch_detected() {
        let st = DownloadState {
            version: STATE_VERSION,
            urls: vec![],
            output: PathBuf::from("x"),
            total_size: Some(1000),
            etag: Some("\"abc\"".into()),
            last_modified: None,
            supports_ranges: true,
            connections: 4,
            segments: vec![],
        };
        assert!(st.validators_match(Some(1000), Some("\"abc\""), None));
        assert!(!st.validators_match(Some(2000), Some("\"abc\""), None));
        assert!(!st.validators_match(Some(1000), Some("\"zzz\""), None));
        // Unknown fresh validators do not disqualify a resume.
        assert!(st.validators_match(None, None, None));
    }

    #[test]
    fn completeness() {
        let seg = SegmentState {
            index: 0,
            start: 0,
            end: 9,
            downloaded: 10,
        };
        assert!(seg.is_complete());
        assert_eq!(seg.len(), 10);
        assert_eq!(seg.cursor(), 10);
    }
}
