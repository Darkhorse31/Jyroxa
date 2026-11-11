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

    /// The remembered best connection count for a host, if any.
    pub fn best_connections(&self, host: &str) -> Option<usize> {
        let s = self.store.lock().unwrap();
        s.hosts.get(host).and_then(|r| {
            if r.best_connections > 0 {
                Some(r.best_connections)
            } else {
                None
            }
        })
    }

    /// The remembered host cap for a host (from throttle events), if any.
    pub fn host_cap(&self, host: &str) -> Option<usize> {
        let s = self.store.lock().unwrap();
        s.hosts.get(host).and_then(|r| {
            if r.host_cap > 0 {
                Some(r.host_cap)
            } else {
                None
            }
        })
    }

    /// Record a throttle event: update the per-host cap.
    pub fn record_throttle(&self, host: &str, cap: usize) {
        {
            let mut s = self.store.lock().unwrap();
            let rec = s.hosts.entry(host.to_string()).or_default();
            if rec.host_cap == 0 || cap < rec.host_cap {
                rec.host_cap = cap;
            }
            rec.last_throttle = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
        let _ = self.persist();
    }

    /// Record an observed throughput for a host at a given connection count,
    /// keeping whichever count produced the highest throughput.
    pub fn observe(&self, host: &str, connections: usize, throughput: f64) {
        {
            let mut s = self.store.lock().unwrap();
            let rec = s.hosts.entry(host.to_string()).or_default();
            if throughput > rec.best_throughput {
                rec.best_throughput = throughput;
                rec.best_connections = connections;
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

    /// Default location for the shared tuning file: `.swiftget/hoststats.json`
    /// under the current working directory (kept inside the project tree).
    pub fn default_path() -> PathBuf {
        Path::new(".swiftget").join("hoststats.json")
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
