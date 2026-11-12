//! Per-host connection auto-tuning memory.
//!
//! The engine remembers the connection count that gave the best throughput for
//! each host so future downloads from that host can start near the sweet spot
//! instead of re-discovering it every time. Optionally persisted as JSON.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Per-host throttle caps expire this long after the last throttle event.
/// Throttling is usually short-lived (rate windows, busy mirrors), and a stale
/// cap would pin every later download from the host to too few connections.
pub const HOST_CAP_DECAY_SECS: u64 = 60 * 60;
/// A remembered best connection count is only trusted for this long.
pub const BEST_DECAY_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostRecord {
    /// Best observed connection count.
    pub best_connections: usize,
    /// Throughput (bytes/sec) observed at that count.
    pub best_throughput: f64,
    /// Per-host connection cap from throttle events (0 = no cap).
    #[serde(default)]
    pub host_cap: usize,
    /// Unix timestamp of the last throttle event (0 = never).
    #[serde(default)]
    pub last_throttle: u64,
    /// Unix timestamp of the last `best_*` update (0 = unknown / legacy).
    #[serde(default)]
    pub best_updated: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    hosts: HashMap<String, HostRecord>,
}

/// Thread-safe store of per-host tuning records.
pub struct HostStats {
    path: Option<PathBuf>,
    store: Mutex<Store>,
}

impl HostStats {
    /// An in-memory-only store (used by tests and ephemeral runs).
    pub fn in_memory() -> Self {
        HostStats {
            path: None,
            store: Mutex::new(Store::default()),
        }
    }

    /// A store backed by a JSON file. Missing/corrupt files start empty.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let store = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Store>(&b).ok())
            .unwrap_or_default();
        HostStats {
            path: Some(path),
            store: Mutex::new(store),
        }
    }

    /// The remembered best connection count for a host, if it is recent.
    pub fn best_connections(&self, host: &str) -> Option<usize> {
        let s = self.store.lock().unwrap();
        s.hosts.get(host).and_then(|r| {
            if r.best_connections > 0 && Self::best_still_valid(r) {
                Some(r.best_connections)
            } else {
                None
            }
        })
    }

    fn best_still_valid(rec: &HostRecord) -> bool {
        rec.best_updated != 0
            && Self::now_secs().saturating_sub(rec.best_updated) <= BEST_DECAY_SECS
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn cap_still_valid(rec: &HostRecord) -> bool {
        if rec.host_cap == 0 {
            return false;
        }
        // Legacy records carry a cap without a timestamp; they can't be dated,
        // so they are treated as expired instead of valid forever.
        if rec.last_throttle == 0 {
            return false;
        }
        Self::now_secs().saturating_sub(rec.last_throttle) <= HOST_CAP_DECAY_SECS
    }

    /// The remembered host cap for a host (from throttle events), if any.
    pub fn host_cap(&self, host: &str) -> Option<usize> {
        let mut s = self.store.lock().unwrap();
        let rec = s.hosts.get_mut(host)?;
        if !Self::cap_still_valid(rec) {
            rec.host_cap = 0;
            rec.last_throttle = 0;
            return None;
        }
        Some(rec.host_cap)
    }

    /// Record a throttle event: update the per-host cap (429/503 only).
    pub fn record_throttle(&self, host: &str, cap: usize) {
        if cap == 0 {
            return;
        }
        {
            let mut s = self.store.lock().unwrap();
            let rec = s.hosts.entry(host.to_string()).or_default();
            if rec.host_cap == 0 || cap < rec.host_cap || !Self::cap_still_valid(rec) {
                rec.host_cap = cap;
            }
            rec.last_throttle = Self::now_secs();
        }
        let _ = self.persist();
    }

    /// Record an observed throughput for a host at a given connection count,
    /// keeping whichever count produced the highest throughput.
    pub fn observe(&self, host: &str, connections: usize, throughput: f64) {
        {
            let mut s = self.store.lock().unwrap();
            let rec = s.hosts.entry(host.to_string()).or_default();
            // Replace the record when it is faster, or when the old one is stale
            // (networks change; an old best is not evidence any more).
            if throughput > rec.best_throughput || !Self::best_still_valid(rec) {
                rec.best_throughput = throughput;
                rec.best_connections = connections;
                rec.best_updated = Self::now_secs();
            }
        }
        // Best-effort persistence; tuning data is advisory.
        let _ = self.persist();
    }

    fn persist(&self) -> Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let s = self.store.lock().unwrap();
            let json = serde_json::to_vec_pretty(&*s)?;
            std::fs::write(path, json)?;
        }
        Ok(())
    }

    /// Default location for the shared tuning file: `.jyroxa/hoststats.json`
    /// under the current working directory (kept inside the project tree).
    pub fn default_path() -> PathBuf {
        Path::new(".jyroxa").join("hoststats.json")
    }

    /// Open the default on-disk store, reading legacy `.swiftget/hoststats.json` if present.
    pub fn with_default_file() -> Self {
        let primary = Self::default_path();
        if primary.is_file() {
            return Self::with_file(primary);
        }
        let legacy = Path::new(".swiftget").join("hoststats.json");
        if legacy.is_file() {
            return Self::with_file(legacy);
        }
        Self::with_file(primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_best() {
        let hs = HostStats::in_memory();
        assert_eq!(hs.best_connections("example.com"), None);
        hs.observe("example.com", 4, 1000.0);
        assert_eq!(hs.best_connections("example.com"), Some(4));
        hs.observe("example.com", 8, 2000.0);
        assert_eq!(hs.best_connections("example.com"), Some(8));
        // Lower throughput does not override the best.
        hs.observe("example.com", 16, 500.0);
        assert_eq!(hs.best_connections("example.com"), Some(8));
    }

    #[test]
    fn throttle_cap_expires_after_decay() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hs = HostStats::in_memory();
        hs.record_throttle("host.test", 4);
        assert_eq!(hs.host_cap("host.test"), Some(4));
        {
            let mut s = hs.store.lock().unwrap();
            let rec = s.hosts.get_mut("host.test").unwrap();
            rec.last_throttle = now.saturating_sub(HOST_CAP_DECAY_SECS + 1);
        }
        assert_eq!(hs.host_cap("host.test"), None);
    }

    #[test]
    fn legacy_cap_without_timestamp_is_ignored() {
        // Exactly what an older build left behind for proof.ovh.net.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hoststats.json");
        std::fs::write(
            &p,
            r#"{"hosts":{"proof.ovh.net":{"best_connections":1,"best_throughput":1.0,"host_cap":1}}}"#,
        )
        .unwrap();
        let hs = HostStats::with_file(&p);
        assert_eq!(hs.host_cap("proof.ovh.net"), None);
        // Undated best is not trusted either.
        assert_eq!(hs.best_connections("proof.ovh.net"), None);
    }

    #[test]
    fn throttle_cap_expires_after_an_hour() {
        let hs = HostStats::in_memory();
        hs.record_throttle("h.test", 2);
        assert_eq!(hs.host_cap("h.test"), Some(2));
        {
            let mut s = hs.store.lock().unwrap();
            let rec = s.hosts.get_mut("h.test").unwrap();
            rec.last_throttle = HostStats::now_secs() - 30 * 60;
        }
        assert_eq!(hs.host_cap("h.test"), Some(2), "still valid at 30 min");
        {
            let mut s = hs.store.lock().unwrap();
            let rec = s.hosts.get_mut("h.test").unwrap();
            rec.last_throttle = HostStats::now_secs() - HOST_CAP_DECAY_SECS - 1;
        }
        assert_eq!(hs.host_cap("h.test"), None);
    }

    #[test]
    fn stale_best_is_replaced() {
        let hs = HostStats::in_memory();
        hs.observe("h.test", 16, 50.0e6);
        {
            let mut s = hs.store.lock().unwrap();
            let rec = s.hosts.get_mut("h.test").unwrap();
            rec.best_updated = HostStats::now_secs() - BEST_DECAY_SECS - 1;
        }
        assert_eq!(hs.best_connections("h.test"), None);
        hs.observe("h.test", 4, 5.0e6);
        assert_eq!(hs.best_connections("h.test"), Some(4));
    }

    #[test]
    fn persists_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hoststats.json");
        {
            let hs = HostStats::with_file(&p);
            hs.observe("host.test", 6, 1234.0);
        }
        let hs2 = HostStats::with_file(&p);
        assert_eq!(hs2.best_connections("host.test"), Some(6));
    }
}
