//! librqbit session wrapper tuned for throughput.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use librqbit::limits::LimitsConfig;
use librqbit::ManagedTorrentHandle;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerMode,
    ListenerOptions, PeerConnectionOptions, Session, SessionOptions, SessionPersistenceConfig,
    TorrentStats,
};
use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::torrent::config::TorrentTuning;
use crate::torrent::magnet::{is_magnet_uri, magnet_trackers, magnet_with_trackers};
use crate::torrent::trackers::{builtin_trackers, merge_tracker_lists};

/// User-visible torrent job progress.
#[derive(Clone, Debug, Default)]
pub struct TorrentSnapshot {
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: f64,
    pub uploaded: u64,
    pub live_peers: u32,
    pub live_seeders: u32,
    pub finished: bool,
    pub state: String,
    pub error: Option<String>,
}

impl From<&TorrentStats> for TorrentSnapshot {
    fn from(s: &TorrentStats) -> Self {
        let speed_bps = s
            .live
            .as_ref()
            .map(|l| l.download_speed.as_bytes() as f64)
            .unwrap_or(0.0);
        let (live_peers, live_seeders) = s
            .live
            .as_ref()
            .map(|l| {
                (
                    l.snapshot.peer_stats.live,
                    l.snapshot.peer_stats.live_seeders,
                )
            })
            .unwrap_or((0, 0));
        Self {
            downloaded: s.progress_bytes,
            total: s.total_bytes,
            speed_bps,
            uploaded: s.uploaded_bytes,
            live_peers,
            live_seeders,
            finished: s.finished,
            state: format!("{}", s.state),
            error: s.error.clone(),
        }
    }
}

pub enum TorrentSource<'a> {
    Url(&'a str),
    FileBytes(Bytes),
}

/// Opaque torrent job handle for UIs (avoids linking librqbit in cdylibs).
#[derive(Clone)]
pub struct TorrentHandle(pub Arc<ManagedTorrentHandle>);

impl TorrentHandle {
    pub fn inner(&self) -> &Arc<ManagedTorrentHandle> {
        &self.0
    }
}

pub struct TorrentJobOptions {
    pub output_folder: Option<PathBuf>,
    pub only_files: Option<Vec<usize>>,
    pub overwrite: bool,
    pub extra_trackers: Vec<String>,
}

impl Default for TorrentJobOptions {
    fn default() -> Self {
        Self {
            output_folder: None,
            only_files: None,
            overwrite: true,
            extra_trackers: Vec::new(),
        }
    }
}

pub struct TorrentSession {
    inner: Arc<Session>,
    tuning: TorrentTuning,
    output_root: PathBuf,
}

impl TorrentSession {
    #[allow(clippy::field_reassign_with_default)]
    pub async fn open(output_root: PathBuf, tuning: TorrentTuning) -> Result<Self> {
        let mut opts = SessionOptions::default();
        opts.fastresume = tuning.fastresume;
        opts.peer_limit = Some(tuning.peer_limit);
        opts.client_name_and_version = Some(format!("Jyroxa {}", env!("CARGO_PKG_VERSION")));
        opts.disable_local_service_discovery = false;
        // Ephemeral DHT per session folder: global default persistence locks across
        // concurrent jyroxa.exe sidecars and breaks parallel downloads / meta probes.
        opts.dht = Some(DhtSessionConfig {
            persistence: None,
            ..Default::default()
        });
        opts.disable_trackers = false;

        if tuning.merge_builtin_trackers {
            for tr in builtin_trackers() {
                if let Ok(url) = url::Url::parse(&tr) {
                    opts.trackers.insert(url);
                }
            }
        }

        let mut listen = ListenerOptions::default();
        listen.mode = ListenerMode::TcpAndUtp;
        listen.enable_upnp_port_forwarding = true;
        listen.max_pending_incoming_handshake_checks = 256;
        opts.listen = Some(listen);

        let mut limits = LimitsConfig::default();
        if tuning.download_limit_bps > 0 {
            limits.download_bps =
                NonZeroU32::new(tuning.download_limit_bps.min(u32::MAX as u64) as u32);
        }
        if tuning.upload_limit_bps > 0 {
            limits.upload_bps =
                NonZeroU32::new(tuning.upload_limit_bps.min(u32::MAX as u64) as u32);
        }
        opts.ratelimits = limits;

        if tuning.fastresume {
            if let Some(dir) = &tuning.session_data_dir {
                let _ = std::fs::create_dir_all(dir);
                opts.persistence = Some(SessionPersistenceConfig::Json {
                    folder: Some(dir.clone()),
                });
            }
        }

        let session = Session::new_with_opts(output_root.clone(), opts)
            .await
            .map_err(|e| Error::other(e.to_string()))?;

        Ok(Self {
            inner: session,
            tuning,
            output_root,
        })
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.inner
    }

    #[allow(clippy::field_reassign_with_default)]
    pub async fn add(
        &self,
        source: TorrentSource<'_>,
        job: TorrentJobOptions,
    ) -> Result<TorrentHandle> {
        let mut merged_trackers = job.extra_trackers;
        let add = match source {
            TorrentSource::Url(url) => {
                let url = url.trim();
                if is_magnet_uri(url) {
                    let from_magnet = magnet_trackers(url);
                    let merged = merge_tracker_lists(&from_magnet, &builtin_trackers());
                    let magnet = magnet_with_trackers(url, &merged)?;
                    merged_trackers = merge_tracker_lists(&merged, &merged_trackers);
                    AddTorrent::from_url(magnet)
                } else {
                    AddTorrent::from_url(url)
                }
            }
            TorrentSource::FileBytes(b) => AddTorrent::from_bytes(b),
        };

        let mut peer_opts = PeerConnectionOptions::default();
        peer_opts.connect_timeout = Some(Duration::from_secs(15));
        peer_opts.read_write_timeout = Some(Duration::from_secs(60));
        peer_opts.keep_alive_interval = Some(Duration::from_secs(30));

        let add_opts = AddTorrentOptions {
            overwrite: job.overwrite,
            only_files: job.only_files,
            output_folder: job
                .output_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            peer_opts: Some(peer_opts),
            peer_limit: Some(self.tuning.peer_limit),
            trackers: if merged_trackers.is_empty() {
                None
            } else {
                Some(merged_trackers)
            },
            force_tracker_interval: Some(Duration::from_secs(120)),
            ..Default::default()
        };

        let resp = self
            .inner
            .add_torrent(add, Some(add_opts))
            .await
            .map_err(|e| Error::other(e.to_string()))?;

        let handle = match resp {
            AddTorrentResponse::Added(_, h) | AddTorrentResponse::AlreadyManaged(_, h) => h,
            AddTorrentResponse::ListOnly(_) => {
                return Err(Error::other("torrent list-only response unexpected"));
            }
        };

        Ok(TorrentHandle(Arc::new(handle)))
    }

    pub async fn pause(&self, handle: &TorrentHandle) -> Result<()> {
        self.inner
            .pause(handle.inner())
            .await
            .map_err(|e| Error::other(e.to_string()))
    }

    pub async fn unpause(&self, handle: &TorrentHandle) -> Result<()> {
        self.inner
            .unpause(handle.inner())
            .await
            .map_err(|e| Error::other(e.to_string()))
    }

    pub fn stats(handle: &TorrentHandle) -> TorrentSnapshot {
        TorrentSnapshot::from(&handle.0.stats())
    }

    /// Poll until completed, seeding limit hit, or `cancel` becomes true.
    pub async fn run_until_done(
        &self,
        handle: TorrentHandle,
        mut cancel: watch::Receiver<bool>,
        on_tick: impl Fn(TorrentSnapshot) + Send + Sync + 'static,
    ) -> Result<()> {
        let ratio_limit = self.tuning.ratio_limit;
        let allow_seed = self.tuning.allow_seeding;

        loop {
            if *cancel.borrow() {
                return Err(Error::Cancelled);
            }
            let snap = Self::stats(&handle.clone());
            on_tick(snap.clone());

            if snap.finished {
                if !allow_seed {
                    return Ok(());
                }
                if ratio_limit > 0.0 && snap.downloaded > 0 {
                    let ratio = snap.uploaded as f64 / snap.downloaded as f64;
                    if ratio >= ratio_limit {
                        return Ok(());
                    }
                } else if ratio_limit <= 0.0 {
                    return Ok(());
                }
            }

            if snap.error.is_some() {
                return Err(Error::other(
                    snap.error.unwrap_or_else(|| "torrent error".into()),
                ));
            }

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                _ = cancel.changed() => {}
            }
        }
    }

    pub async fn wait_completed(&self, handle: &TorrentHandle) -> Result<()> {
        handle
            .0
            .wait_until_completed()
            .await
            .map_err(|e| Error::other(e.to_string()))
    }

    pub fn output_root(&self) -> &PathBuf {
        &self.output_root
    }

    pub fn tuning(&self) -> &TorrentTuning {
        &self.tuning
    }
}
