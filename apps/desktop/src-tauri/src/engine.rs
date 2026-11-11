//! Persistent download queue and swiftget-core bridge for the desktop shell.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pub max_connections_per_download: usize,
    pub per_host_cap: usize,
    pub global_speed_limit: Option<u64>,
    #[serde(alias = "default_folder")]
    pub download_dir: String,
    pub category_dirs: std::collections::HashMap<String, String>,
    pub clipboard_monitor: bool,
    pub autotune: bool,
    pub start_minimized: bool,
    pub notifications: bool,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub prevent_sleep: bool,
    pub auto_resume: bool,
    pub theme: String,
    pub scheduler_start: Option<String>,
    pub scheduler_stop: Option<String>,
    pub scheduler_days: Vec<u8>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            max_concurrent: 3,
            max_connections_per_download: 8,
            per_host_cap: 16,
            global_speed_limit: None,
            download_dir: default_download_dir(),
            category_dirs: std::collections::HashMap::new(),
            clipboard_monitor: true,
            autotune: true,
            start_minimized: false,
            notifications: true,
            close_to_tray: true,
            start_with_windows: false,
            prevent_sleep: true,
            auto_resume: true,
            theme: "system".into(),
            scheduler_start: None,
            scheduler_stop: None,
            scheduler_days: vec![0, 1, 2, 3, 4, 5, 6],
        }
    }
}

fn default_download_dir() -> String {
    std::env::var("USERPROFILE")
        .map(|h| format!(r"{h}\Downloads"))
        .unwrap_or_else(|_| ".".into())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
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
    #[serde(default)]
    pub status: TaskStatus,
}

struct InnerItem {
    view: DownloadView,
    control: Option<Control>,
}

enum ResumeAction {
    Live,
    Respawn,
}

pub struct DesktopEngine {
    downloader: Arc<RwLock<Downloader>>,
    linkgrabber: Arc<LinkGrabber>,
    items: Arc<Mutex<HashMap<u64, InnerItem>>>,
    pending: Arc<Mutex<VecDeque<u64>>>,
    active_count: Arc<Mutex<usize>>,
    next_id: AtomicU64,
    settings: Arc<Mutex<AppSettings>>,
    /// Cached `close_to_tray` for synchronous window close handling (no async lock).
    close_to_tray_cached: AtomicBool,
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

        let close_to_tray = settings.close_to_tray;
        crate::platform::set_close_to_tray(close_to_tray);
        let engine = Arc::new_cyclic(|weak| DesktopEngine {
            downloader,
            linkgrabber,
            items: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            active_count: Arc::new(Mutex::new(0)),
            next_id: AtomicU64::new(next_id),
            settings: Arc::new(Mutex::new(settings)),
            close_to_tray_cached: AtomicBool::new(close_to_tray),
            data_dir,
            weak: weak.clone(),
        });
        Ok(engine)
    }

    #[allow(dead_code)]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    #[allow(dead_code)]
    pub fn close_to_tray(&self) -> bool {
        self.close_to_tray_cached.load(Ordering::Relaxed)
    }

    pub async fn get_settings(&self) -> AppSettings {
        self.settings.lock().await.clone()
    }

    pub async fn set_settings(
        &self,
        s: AppSettings,
        app_name: &str,
        exe: Option<&Path>,
    ) -> CoreResult<()> {
        {
            let mut settings = self.settings.lock().await;
            *settings = s.clone();
        }
        self.close_to_tray_cached
            .store(s.close_to_tray, Ordering::Release);
        crate::platform::set_close_to_tray(s.close_to_tray);
        {
            let mut dl = self.downloader.write().await;
            dl.set_global_limit(s.global_speed_limit);
            let mut cfg = dl.config().clone();
            cfg.max_connections = s.per_host_cap.min(swiftget_core::MAX_CONNECTIONS);
            cfg.autotune = s.autotune;
            dl.set_config(cfg);
        }
        if let Some(exe) = exe {
            crate::platform::set_run_at_startup(
                app_name,
                exe,
                s.start_with_windows,
                s.start_minimized,
            );
        }
        self.save_settings(&s)?;
        Ok(())
    }

    pub async fn default_category_dirs(
        &self,
        product_label: &str,
    ) -> std::collections::HashMap<String, String> {
        let download_dir = self.settings.lock().await.download_dir.clone();
        let base = PathBuf::from(download_dir);
        let root = base.join(product_label);
        let mut map = std::collections::HashMap::new();
        for (key, folder) in [
            ("Video", "Video"),
            ("Music", "Music"),
            ("Documents", "Documents"),
            ("Compressed", "Compressed"),
            ("Programs", "Programs"),
            ("Other", "Other"),
        ] {
            map.insert(key.into(), root.join(folder).to_string_lossy().into_owned());
        }
        map
    }

    pub fn suggest_save_path(&self, dir: &str, name: &str) -> String {
        swiftget_core::dedupe_path(dir, name)
            .to_string_lossy()
            .into_owned()
    }

    pub async fn probe_url(&self, url: &str) -> CoreResult<swiftget_core::ProbeInfo> {
        let client = build_client()?;
        swiftget_core::probe_url(&client, url, std::time::Duration::from_secs(10)).await
    }

    pub async fn pause_many(&self, app: AppHandle, ids: &[u64]) -> usize {
        let mut n = 0usize;
        for &id in ids {
            if self.pause_and_mark(app.clone(), id).await {
                n += 1;
            }
        }
        n
    }

    pub async fn resume_many(&self, app: AppHandle, ids: &[u64]) -> usize {
        let mut n = 0usize;
        for &id in ids {
            if self.resume(app.clone(), id).await {
                n += 1;
            }
        }
        n
    }

    pub async fn cancel_many(&self, app: AppHandle, ids: &[u64]) -> usize {
        let mut n = 0usize;
        for &id in ids {
            if self.cancel(app.clone(), id).await {
                n += 1;
            }
        }
        n
    }

    pub async fn remove_download(&self, app: AppHandle, id: u64, delete_file: bool) -> bool {
        let _ = self.cancel(app.clone(), id).await;
        let output = {
            let mut items = self.items.lock().await;
            let Some(inner) = items.remove(&id) else {
                return false;
            };
            inner.view.output.clone()
        };
        if delete_file {
            let path = PathBuf::from(&output);
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("part"));
            let state = path.with_extension("state.json");
            let _ = std::fs::remove_file(state);
        }
        let _ = app.emit("downloads-changed", ());
        let _ = self.save_queue_snapshot().await;
        true
    }

    pub async fn remove_many(&self, app: AppHandle, ids: &[u64], delete_file: bool) -> usize {
        let mut n = 0usize;
        for &id in ids {
            if self.remove_download(app.clone(), id, delete_file).await {
                n += 1;
            }
        }
        n
    }

    pub async fn retry_download(&self, app: AppHandle, id: u64) -> bool {
        let args = {
            let mut items = self.items.lock().await;
            let Some(inner) = items.get_mut(&id) else {
                return false;
            };
            if !matches!(
                inner.view.status,
                TaskStatus::Failed | TaskStatus::Cancelled
            ) {
                return false;
            }
            inner.view.status = TaskStatus::Queued;
            inner.view.error = None;
            inner.view.downloaded = 0;
            inner.view.speed_bps = 0.0;
            inner.control = None;
            AddDownloadArgs {
                urls: inner.view.urls.clone(),
                output_dir: PathBuf::from(&inner.view.output)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(&inner.view.output)
                    .to_string(),
                filename: inner.view.filename.clone(),
                connections: inner.view.connections,
                connections_auto: inner.view.connections_auto,
                sha256: inner.view.sha256.clone(),
                md5: inner.view.md5.clone(),
            }
        };
        let settings = self.settings.lock().await.clone();
        self.enqueue(id).await;
        self.pump_queue(app.clone(), settings.max_concurrent).await;
        let _ = self.save_queue_snapshot().await;
        let _ = app.emit("downloads-changed", ());
        let _ = args; // fields already applied on existing row
        true
    }

    pub async fn pause_all(&self, app: AppHandle) -> usize {
        let ids: Vec<u64> = {
            let items = self.items.lock().await;
            items
                .values()
                .filter(|i| matches!(i.view.status, TaskStatus::Running | TaskStatus::Starting))
                .map(|i| i.view.id)
                .collect()
        };
        self.pause_many(app, &ids).await
    }

    pub async fn resume_all(&self, app: AppHandle) -> usize {
        let ids: Vec<u64> = {
            let items = self.items.lock().await;
            items
                .values()
                .filter(|i| i.view.status == TaskStatus::Paused)
                .map(|i| i.view.id)
                .collect()
        };
        self.resume_many(app, &ids).await
    }

    pub async fn tray_snapshot(&self) -> (usize, f64, f64) {
        let items = self.items.lock().await;
        let mut active = 0usize;
        let mut pct_sum = 0.0f64;
        let mut pct_count = 0usize;
        let mut speed = 0.0f64;
        for inner in items.values() {
            if matches!(
                inner.view.status,
                TaskStatus::Running | TaskStatus::Starting
            ) {
                active += 1;
                speed += inner.view.speed_bps;
                if let Some(total) = inner.view.total.filter(|&t| t > 0) {
                    pct_sum += (inner.view.downloaded as f64 / total as f64) * 100.0;
                    pct_count += 1;
                }
            }
        }
        let pct = if pct_count > 0 {
            pct_sum / pct_count as f64
        } else {
            0.0
        };
        (active, pct, speed)
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    fn queue_path(&self) -> PathBuf {
        self.data_dir.join("queue.json")
    }

    fn schedule_queue_persist(engine: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let _ = engine.save_queue_snapshot().await;
        });
    }

    fn delete_download_artifacts(output: &str) {
        let path = PathBuf::from(output);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("part"));
        let _ = std::fs::remove_file(swiftget_core::state::DownloadState::state_path(&path));
    }

    fn progress_from_state(output: &Path) -> (u64, Option<u64>, Vec<SegmentState>) {
        let state_path = swiftget_core::state::DownloadState::state_path(output);
        match swiftget_core::state::DownloadState::load(&state_path) {
            Ok(Some(st)) => {
                let downloaded = st.segments.iter().map(|s| s.downloaded).sum();
                (downloaded, st.total_size, st.segments)
            }
            _ => (0, None, Vec::new()),
        }
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
                    status: v.status.clone(),
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
        let settings = self.settings.lock().await.clone();
        let mut restored_paused = false;
        for item in q.items {
            let output = PathBuf::from(&item.output);
            let filename = output
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("download.bin")
                .to_string();
            let status = match item.status {
                TaskStatus::Running | TaskStatus::Starting => TaskStatus::Queued,
                other => other,
            };
            let (downloaded, total, segments) = if status == TaskStatus::Paused {
                Self::progress_from_state(&output)
            } else {
                (0, None, Vec::new())
            };
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let connections = if item.connections_auto {
                8
            } else {
                item.connections.clamp(1, swiftget_core::MAX_CONNECTIONS)
            };
            let view = DownloadView {
                id,
                urls: item.urls.clone(),
                filename: filename.clone(),
                output: item.output.clone(),
                connections,
                connections_auto: item.connections_auto,
                sha256: item.sha256.clone(),
                md5: item.md5.clone(),
                status: status.clone(),
                downloaded,
                total,
                speed_bps: 0.0,
                segments,
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
            match status {
                TaskStatus::Paused => restored_paused = true,
                TaskStatus::Queued => self.enqueue(id).await,
                _ => {}
            }
        }
        let _ = app.emit("downloads-changed", ());
        self.pump_queue(app.clone(), settings.max_concurrent).await;
        if restored_paused {
            let _ = self.save_queue_snapshot().await;
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

            let notifications_on = settings_lock.lock().await.notifications;
            let mut native_toast: Option<(String, String)> = None;
            {
                let mut items = engine.items.lock().await;
                if let Some(inner) = items.get_mut(&id) {
                    match &result {
                        Ok(out) => {
                            inner.view.status = TaskStatus::Completed;
                            inner.view.downloaded = out.total_size;
                            inner.view.total = Some(out.total_size);
                            inner.view.speed_bps = 0.0;
                            let finished = DownloadFinishedPayload {
                                id,
                                path: inner.view.output.clone(),
                                filename: inner.view.filename.clone(),
                                total: out.total_size,
                            };
                            let _ = app.emit("download-finished", finished);
                            if notifications_on {
                                native_toast = Some((
                                    "Download complete".into(),
                                    inner.view.filename.clone(),
                                ));
                            }
                        }
                        Err(swiftget_core::Error::Cancelled) => {
                            inner.view.status = TaskStatus::Cancelled;
                        }
                        Err(e) => {
                            inner.view.status = TaskStatus::Failed;
                            inner.view.error = Some(e.to_string());
                            if notifications_on {
                                native_toast = Some((
                                    format!("Download failed: {}", inner.view.filename),
                                    e.to_string(),
                                ));
                            }
                        }
                    }
                    inner.control = None;
                }
            }
            if let Some((title, body)) = native_toast {
                show_native_notification(&app, &title, &body);
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
        let action = {
            let items = self.items.lock().await;
            let Some(inner) = items.get(&id) else {
                return false;
            };
            if let Some(c) = &inner.control {
                c.resume();
                Some(ResumeAction::Live)
            } else if inner.view.status == TaskStatus::Paused {
                Some(ResumeAction::Respawn)
            } else {
                None
            }
        };
        let Some(action) = action else {
            return false;
        };
        match action {
            ResumeAction::Live => {
                let mut items = self.items.lock().await;
                if let Some(inner) = items.get_mut(&id) {
                    inner.view.status = TaskStatus::Running;
                }
            }
            ResumeAction::Respawn => {
                {
                    let mut items = self.items.lock().await;
                    if let Some(inner) = items.get_mut(&id) {
                        inner.view.status = TaskStatus::Queued;
                    }
                }
                let max = self.settings.lock().await.max_concurrent;
                self.enqueue(id).await;
                self.pump_queue(app.clone(), max).await;
            }
        }
        let _ = app.emit("downloads-changed", ());
        let engine = self.weak.upgrade();
        if let Some(engine) = engine {
            Self::schedule_queue_persist(engine);
        }
        true
    }

    pub async fn cancel(&self, app: AppHandle, id: u64) -> bool {
        let (cancelled, output, purge_files) = {
            let items = self.items.lock().await;
            let Some(inner) = items.get(&id) else {
                return false;
            };
            let purge_files = inner.control.is_none();
            let cancelled = if let Some(c) = &inner.control {
                c.cancel();
                true
            } else {
                matches!(
                    inner.view.status,
                    TaskStatus::Queued | TaskStatus::Starting | TaskStatus::Paused
                )
            };
            (cancelled, inner.view.output.clone(), purge_files)
        };
        if !cancelled {
            return false;
        }
        {
            let mut items = self.items.lock().await;
            if let Some(inner) = items.get_mut(&id) {
                if !matches!(inner.view.status, TaskStatus::Cancelled) {
                    inner.view.status = TaskStatus::Cancelled;
                }
            }
        }
        {
            let mut pending = self.pending.lock().await;
            pending.retain(|&x| x != id);
        }
        if purge_files {
            Self::delete_download_artifacts(&output);
        }
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
        if let Some(engine) = self.weak.upgrade() {
            Self::schedule_queue_persist(engine);
        }
        true
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
            if let Some(engine) = self.weak.upgrade() {
                Self::schedule_queue_persist(engine);
            }
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

fn show_native_notification(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub id: u64,
    pub event: ProgressEvent,
}

#[derive(Clone, Serialize)]
pub struct DownloadFinishedPayload {
    pub id: u64,
    pub path: String,
    pub filename: String,
    pub total: u64,
}

async fn apply_event(items: &Mutex<HashMap<u64, InnerItem>>, id: u64, ev: &ProgressEvent) {
    let mut guard = items.lock().await;
    let Some(inner) = guard.get_mut(&id) else {
        return;
    };
    if !matches!(inner.view.status, TaskStatus::Paused) {
        inner.view.status = TaskStatus::Running;
    }
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
    if engine.get_settings().await.auto_resume {
        engine.restore_queue(app.clone()).await?;
    }
    Ok(())
}

pub fn clipboard_url_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"https?://\S+").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_status_defaults_to_queued() {
        let json = r#"{"urls":["http://x"],"output":"C:\\f.bin","connections":4,"connections_auto":false,"sha256":null,"md5":null}"#;
        let item: PersistedItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.status, TaskStatus::Queued);
    }

    #[test]
    fn persisted_pause_roundtrip() {
        let item = PersistedItem {
            urls: vec!["http://x".into()],
            output: r"C:\f.bin".into(),
            connections: 4,
            connections_auto: false,
            sha256: None,
            md5: None,
            status: TaskStatus::Paused,
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: PersistedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, TaskStatus::Paused);
    }
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
