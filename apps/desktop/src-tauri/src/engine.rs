//! Persistent download queue and swiftget-core bridge for the desktop shell.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};
use swiftget_core::download::{Control, DownloadConfig, DownloadRequest, Downloader};
use swiftget_core::events::ProgressEvent;
use swiftget_core::linkgrabber::{GrabOptions, LinkGrabber, LinkInfo, Package};
use swiftget_core::state::SegmentState;
use swiftget_core::{build_client, HostStats, Result as CoreResult};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub max_concurrent: usize,
    pub global_speed_limit: Option<u64>,
    pub default_folder: String,
    pub clipboard_monitor: bool,
    pub autotune: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            max_concurrent: 3,
            global_speed_limit: None,
            default_folder: default_download_dir(),
            clipboard_monitor: true,
            autotune: true,
        }
    }
}

fn default_download_dir() -> String {
    std::env::var("USERPROFILE")
        .map(|h| format!(r"{h}\Downloads"))
        .unwrap_or_else(|_| ".".into())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Starting,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadView {
    pub id: u64,
    pub urls: Vec<String>,
    pub filename: String,
    pub output: String,
    pub connections: usize,
    pub connections_auto: bool,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub status: TaskStatus,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bps: f64,
    pub segments: Vec<SegmentState>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddDownloadArgs {
    pub urls: Vec<String>,
    pub output_dir: String,
    pub filename: String,
    pub connections: usize,
    pub connections_auto: bool,
    pub sha256: Option<String>,
    pub md5: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrabOptionsDto {
    pub deep: bool,
    pub include_ext: Option<Vec<String>>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub host_filter: Option<String>,
    pub no_probe: bool,
}

impl From<&GrabOptionsDto> for GrabOptions {
    fn from(d: &GrabOptionsDto) -> Self {
        GrabOptions {
            deep: d.deep,
            include_ext: d.include_ext.clone(),
            min_size: d.min_size,
            max_size: d.max_size,
            host_filter: d.host_filter.clone(),
            concurrency: 16,
            no_probe: d.no_probe,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedQueue {
    pub version: u32,
    pub items: Vec<PersistedItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedItem {
    pub urls: Vec<String>,
    pub output: String,
    pub connections: usize,
    pub connections_auto: bool,
    pub sha256: Option<String>,
    pub md5: Option<String>,
}

struct InnerItem {
    view: DownloadView,
    control: Option<Control>,
}

pub struct DesktopEngine {
    downloader: Arc<RwLock<Downloader>>,
    linkgrabber: Arc<LinkGrabber>,
    items: Arc<Mutex<HashMap<u64, InnerItem>>>,
    pending: Arc<Mutex<VecDeque<u64>>>,
    active_count: Arc<Mutex<usize>>,
    next_id: AtomicU64,
    settings: Arc<Mutex<AppSettings>>,
    data_dir: PathBuf,
    weak: Weak<Self>,
}

impl DesktopEngine {
    pub fn build(data_dir: PathBuf) -> CoreResult<Arc<Self>> {
        let client = build_client()?;
        let host_stats = Arc::new(HostStats::with_file(data_dir.join("hoststats.json")));
        let mut config = DownloadConfig::default();
        config.autotune = true;

        let downloader = Arc::new(RwLock::new(
            Downloader::new(config)?
                .with_client(client.clone())
                .with_host_stats(host_stats),
        ));
        let linkgrabber = Arc::new(LinkGrabber::new(client));

        let mut settings = AppSettings::default();
        let settings_path = data_dir.join("settings.json");
        if let Ok(text) = std::fs::read_to_string(&settings_path) {
            if let Ok(s) = serde_json::from_str(&text) {
                settings = s;
            }
        }
        let mut next_id = 1u64;
        let queue_path = data_dir.join("queue.json");
        if let Ok(text) = std::fs::read_to_string(&queue_path) {
            if let Ok(q) = serde_json::from_str::<PersistedQueue>(&text) {
                next_id = q.items.len() as u64 + 1;
            }
        }

        let engine = Arc::new_cyclic(|weak| DesktopEngine {
            downloader,
            linkgrabber,
            items: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            active_count: Arc::new(Mutex::new(0)),
            next_id: AtomicU64::new(next_id),
            settings: Arc::new(Mutex::new(settings)),
            data_dir,
            weak: weak.clone(),
        });
        Ok(engine)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub async fn get_settings(&self) -> AppSettings {
        self.settings.lock().await.clone()
    }

    pub async fn set_settings(&self, s: AppSettings) -> CoreResult<()> {
        {
            let mut settings = self.settings.lock().await;
            *settings = s.clone();
        }
        self.downloader
            .write()
            .await
            .set_global_limit(s.global_speed_limit);
        self.save_settings(&s)?;
        Ok(())
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    fn queue_path(&self) -> PathBuf {
        self.data_dir.join("queue.json")
    }

    fn save_settings(&self, s: &AppSettings) -> CoreResult<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        let text = serde_json::to_string_pretty(s)?;
        std::fs::write(self.settings_path(), text)?;
        Ok(())
    }

    pub async fn save_queue_snapshot(&self) -> CoreResult<()> {
        let items = self.items.lock().await;
        let mut persisted = Vec::new();
        for inner in items.values() {
            let v = &inner.view;
            if matches!(
                v.status,
                TaskStatus::Queued | TaskStatus::Paused | TaskStatus::Running | TaskStatus::Starting
            ) {
                persisted.push(PersistedItem {
                    urls: v.urls.clone(),
                    output: v.output.clone(),
                    connections: v.connections,
                    connections_auto: v.connections_auto,
                    sha256: v.sha256.clone(),
                    md5: v.md5.clone(),
                });
            }
        }
        let queue = PersistedQueue {
            version: 1,
            items: persisted,
        };
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::write(self.queue_path(), serde_json::to_string_pretty(&queue)?)?;
        Ok(())
    }

    pub async fn restore_queue(&self, app: AppHandle) -> CoreResult<()> {
        let text = match std::fs::read_to_string(self.queue_path()) {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        let q: PersistedQueue = serde_json::from_str(&text)?;
        for item in q.items {
            let output = PathBuf::from(&item.output);
            let filename = output
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("download.bin")
                .to_string();
            let dir = output
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or(&item.output)
                .to_string();
            self.add_download(
                app.clone(),
                AddDownloadArgs {
                    urls: item.urls,
                    output_dir: dir,
                    filename,
                    connections: item.connections,
                    connections_auto: item.connections_auto,
                    sha256: item.sha256,
                    md5: item.md5,
                },
            )
            .await?;
        }
        let _ = std::fs::remove_file(self.queue_path());
        Ok(())
    }

    pub async fn list_downloads(&self) -> Vec<DownloadView> {
        let items = self.items.lock().await;
        let mut out: Vec<_> = items.values().map(|i| i.view.clone()).collect();
        out.sort_by_key(|v| v.id);
        out
    }

    pub async fn add_download(&self, app: AppHandle, args: AddDownloadArgs) -> CoreResult<u64> {
        if args.urls.is_empty() {
            return Err(swiftget_core::Error::other("at least one URL required"));
        }
        let settings = self.settings.lock().await.clone();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let output = PathBuf::from(&args.output_dir).join(&args.filename);
        let connections = if args.connections_auto {
            8
        } else {
            args.connections.clamp(1, swiftget_core::MAX_CONNECTIONS)
        };

        let view = DownloadView {
            id,
            urls: args.urls.clone(),
            filename: args.filename.clone(),
            output: output.to_string_lossy().into_owned(),
            connections,
            connections_auto: args.connections_auto,
            sha256: args.sha256.clone(),
            md5: args.md5.clone(),
            status: TaskStatus::Queued,
            downloaded: 0,
            total: None,
            speed_bps: 0.0,
            segments: Vec::new(),
            error: None,
        };

        {
            let mut items = self.items.lock().await;
            items.insert(
                id,
                InnerItem {
                    view: view.clone(),
                    control: None,
                },
            );
        }

        let _ = app.emit("downloads-changed", ());
        self.enqueue(id).await;
        self.pump_queue(app, settings.max_concurrent).await;
        let _ = self.save_queue_snapshot().await;
        Ok(id)
    }

    async fn enqueue(&self, id: u64) {
        let mut pending = self.pending.lock().await;
        pending.push_back(id);
    }

    async fn pump_queue(&self, app: AppHandle, max_concurrent: usize) {
        let Some(engine) = self.weak.upgrade() else {
            return;
        };
        loop {
            let slot_free = {
                let active = *engine.active_count.lock().await;
                active < max_concurrent.max(1)
            };
            if !slot_free {
                break;
            }
            let next_id = {
                let mut pending = engine.pending.lock().await;
                pending.pop_front()
            };
            let Some(id) = next_id else { break };

            let already_running = {
                let items = engine.items.lock().await;
                items
                    .get(&id)
                    .map(|i| {
                        matches!(
                            i.view.status,
                            TaskStatus::Running | TaskStatus::Starting
                        )
                    })
                    .unwrap_or(false)
            };
            if already_running {
                continue;
            }

            engine.spawn_download(app.clone(), id);
        }
    }

    fn spawn_download(self: &Arc<Self>, app: AppHandle, id: u64) {
        let engine = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let control = Control::new();
            let autotune = engine.settings.lock().await.autotune;
            let speed_limit = engine.settings.lock().await.global_speed_limit;
            let request = {
                let mut items = engine.items.lock().await;
                let Some(inner) = items.get_mut(&id) else {
                    return;
                };
                inner.control = Some(control.clone());
                inner.view.status = TaskStatus::Starting;
                let v = &inner.view;
                DownloadRequest {
                    urls: v.urls.clone(),
                    output: PathBuf::from(&v.output),
                    connections: v.connections,
                    speed_limit,
                    sha256: v.sha256.clone(),
                    md5: v.md5.clone(),
                    autotune,
                }
            };

            let _ = app.emit("downloads-changed", ());
            {
                let mut active = engine.active_count.lock().await;
                *active += 1;
            }

            let downloader = engine.downloader.clone();
            let engine_items = Arc::clone(&engine.items);
            let active_count = Arc::clone(&engine.active_count);
            let settings_lock = Arc::clone(&engine.settings);
            let engine_weak = engine.weak.clone();

            let (tx, mut rx) = swiftget_core::events::channel();
            let app_ev = app.clone();
            let id_ev = id;

            let consumer = tauri::async_runtime::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    apply_event(&engine_items, id_ev, &ev).await;
                    let _ = app_ev.emit(
                        "download-progress",
                        ProgressPayload {
                            id: id_ev,
                            event: ev,
                        },
                    );
                }
            });

            let result = {
                let dl = downloader.read().await;
                dl.download(request, Some(tx), control.clone(), None)
                    .await
            };

            let _ = consumer.await;

            {
                let mut items = engine.items.lock().await;
                if let Some(inner) = items.get_mut(&id) {
                    match &result {
                        Ok(out) => {
                            inner.view.status = TaskStatus::Completed;
                            inner.view.downloaded = out.total_size;
                            inner.view.total = Some(out.total_size);
                            inner.view.speed_bps = 0.0;
                        }
                        Err(swiftget_core::Error::Cancelled) => {
                            inner.view.status = TaskStatus::Cancelled;
                        }
                        Err(e) => {
                            inner.view.status = TaskStatus::Failed;
                            inner.view.error = Some(e.to_string());
                        }
                    }
                    inner.control = None;
                }
            }

            {
                let mut active = active_count.lock().await;
                *active = active.saturating_sub(1);
            }

            let _ = app.emit("downloads-changed", ());

            let max = settings_lock.lock().await.max_concurrent;
            if let Some(engine) = engine_weak.upgrade() {
                let _ = engine.save_queue_snapshot().await;
                engine.pump_queue(app.clone(), max).await;
            }
        });
    }

    pub async fn pause(&self, id: u64) -> bool {
        let items = self.items.lock().await;
        if let Some(inner) = items.get(&id) {
            if let Some(c) = &inner.control {
                c.pause();
                return true;
            }
        }
        false
    }

    pub async fn resume(&self, app: AppHandle, id: u64) -> bool {
        let ok = {
            let items = self.items.lock().await;
            if let Some(inner) = items.get(&id) {
                if let Some(c) = &inner.control {
                    c.resume();
                    true
                } else if inner.view.status == TaskStatus::Queued {
                    false
                } else {
                    false
                }
            } else {
                false
            }
        };
        if ok {
            let mut items = self.items.lock().await;
            if let Some(inner) = items.get_mut(&id) {
                inner.view.status = TaskStatus::Running;
            }
            let _ = app.emit("downloads-changed", ());
        }
        ok
    }

    pub async fn cancel(&self, app: AppHandle, id: u64) -> bool {
        let cancelled = {
            let items = self.items.lock().await;
            if let Some(inner) = items.get(&id) {
                if let Some(c) = &inner.control {
                    c.cancel();
                    true
                } else {
                    matches!(
                        inner.view.status,
                        TaskStatus::Queued | TaskStatus::Starting
                    )
                }
            } else {
                false
            }
        };
        if cancelled {
            let mut items = self.items.lock().await;
            if let Some(inner) = items.get_mut(&id) {
                if !matches!(inner.view.status, TaskStatus::Cancelled) {
                    inner.view.status = TaskStatus::Cancelled;
                }
            }
            let mut pending = self.pending.lock().await;
            pending.retain(|&x| x != id);
            let _ = app.emit("downloads-changed", ());
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    id,
                    event: ProgressEvent::Progress {
                        downloaded: 0,
                        total: None,
                        speed_bps: 0.0,
                        connections: 0,
                        eta_secs: None,
                        avg_speed_bps: None,
                        elapsed_secs: None,
                        active_connections: None,
                        status: Some("cancelled".into()),
                        resumable: None,
                        server: None,
                        protocol: None,
                        segments: None,
                    },
                },
            );
            let _ = self.save_queue_snapshot().await;
        }
        cancelled
    }

    pub async fn pause_and_mark(&self, app: AppHandle, id: u64) -> bool {
        if self.pause(id).await {
            let (downloaded, total) = {
                let mut items = self.items.lock().await;
                let Some(inner) = items.get_mut(&id) else {
                    return false;
                };
                inner.view.status = TaskStatus::Paused;
                (inner.view.downloaded, inner.view.total)
            };
            let _ = app.emit("downloads-changed", ());
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    id,
                    event: ProgressEvent::Progress {
                        downloaded,
                        total,
                        speed_bps: 0.0,
                        connections: 0,
                        eta_secs: None,
                        avg_speed_bps: None,
                        elapsed_secs: None,
                        active_connections: None,
                        status: Some("paused".into()),
                        resumable: Some(true),
                        server: None,
                        protocol: None,
                        segments: None,
                    },
                },
            );
            true
        } else {
            false
        }
    }

    pub async fn grab_links(
        &self,
        input: &str,
        opts: &GrabOptionsDto,
    ) -> CoreResult<Vec<Package>> {
        self.linkgrabber.grab(input, &opts.into()).await
    }

    pub async fn probe_urls(&self, urls: Vec<String>) -> Vec<LinkInfo> {
        let grabber = self.linkgrabber.clone();
        let mut out = Vec::with_capacity(urls.len());
        for url in urls {
            out.push(grabber.probe_link(&url).await);
        }
        out
    }

}

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub id: u64,
    pub event: ProgressEvent,
}

async fn apply_event(items: &Mutex<HashMap<u64, InnerItem>>, id: u64, ev: &ProgressEvent) {
    let mut guard = items.lock().await;
    let Some(inner) = guard.get_mut(&id) else {
        return;
    };
    inner.view.status = TaskStatus::Running;
    match ev {
        ProgressEvent::Started { total_size, .. } => {
            inner.view.total = *total_size;
        }
        ProgressEvent::Progress {
            downloaded,
            total,
            speed_bps,
            connections: _,
            ..
        } => {
            inner.view.downloaded = *downloaded;
            if total.is_some() {
                inner.view.total = *total;
            }
            inner.view.speed_bps = *speed_bps;
        }
        ProgressEvent::SegmentProgress {
            index,
            start,
            current,
            end,
        } => {
            let segs = &mut inner.view.segments;
            if let Some(seg) = segs.iter_mut().find(|s| s.index == *index) {
                seg.start = *start;
                seg.end = *end;
                seg.downloaded = *current;
            } else {
                segs.push(SegmentState {
                    index: *index,
                    start: *start,
                    end: *end,
                    downloaded: *current,
                });
                segs.sort_by_key(|s| s.index);
            }
        }
        ProgressEvent::Failed { error } => {
            inner.view.error = Some(error.clone());
        }
        _ => {}
    }
}

/// Shared Tauri state wrapper (async mutex).
pub struct EngineState(pub Arc<DesktopEngine>);

pub async fn init_engine(app: &AppHandle) -> CoreResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| swiftget_core::Error::other(e.to_string()))?
        .join("swiftget");
    std::fs::create_dir_all(&dir)?;
    let engine = DesktopEngine::build(dir)?;
    let settings = engine.get_settings().await;
    engine
        .downloader
        .write()
        .await
        .set_global_limit(settings.global_speed_limit);
    app.manage(EngineState(engine.clone()));
    engine.restore_queue(app.clone()).await?;
    Ok(())
}

pub fn clipboard_url_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"https?://\S+").unwrap())
}

pub async fn run_clipboard_monitor(app: AppHandle) {
    let mut last_clipboard = String::new();
    let mut last_emitted_url = String::new();
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let state = app.try_state::<EngineState>();
        let Some(state) = state else { continue };
        let settings = state.0.get_settings().await;
        if !settings.clipboard_monitor {
            continue;
        }
        let text = app.clipboard().read_text().unwrap_or_default();
        if text == last_clipboard || text.is_empty() {
            continue;
        }
        last_clipboard = text.clone();
        if let Some(m) = clipboard_url_re().find(&text) {
            let url = m.as_str().to_string();
            if url != last_emitted_url {
                last_emitted_url = url.clone();
                let _ = app.emit("clipboard-url", url);
            }
        }
    }
}
