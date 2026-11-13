mod commands;
mod engine;
mod platform;
mod shell_paths;
mod window_state;

use engine::{init_engine, run_clipboard_monitor, EngineState};
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use window_state::{MonitorRect, WindowState};

fn window_state_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| {
        let primary = d.join("jyroxa").join("window.json");
        if primary.is_file() {
            return primary;
        }
        let legacy = d.join("swiftget").join("window.json");
        if legacy.is_file() {
            return legacy;
        }
        primary
    })
}

fn load_window_state(app: &tauri::AppHandle) -> Option<WindowState> {
    let path = window_state_path(app)?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_window_state(app: &tauri::AppHandle, state: &WindowState) {
    let Some(dir) = app.path().app_data_dir().ok() else {
        return;
    };
    let path = dir.join("jyroxa").join("window.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, text);
    }
}

/// Current geometry of a normal, visible main window. Returns None while the window is
/// minimized or hidden, because Windows reports those as 0x0 at (-32000, -32000).
fn snapshot_window(app: &tauri::AppHandle) -> Option<WindowState> {
    let w = app.get_webview_window("main")?;
    if w.is_minimized().unwrap_or(true) || !w.is_visible().unwrap_or(false) {
        return None;
    }
    if w.is_maximized().unwrap_or(false) {
        // Keep the last normal size so un-maximizing next time still looks right.
        let mut prev = load_window_state(app).filter(|s| s.is_worth_saving()).unwrap_or(WindowState {
            width: window_state::DEFAULT_WIDTH,
            height: window_state::DEFAULT_HEIGHT,
            x: None,
            y: None,
            maximized: false,
        });
        prev.maximized = true;
        return Some(prev);
    }
    let size = w.inner_size().ok()?;
    let pos = w.outer_position().ok();
    let state = WindowState {
        width: size.width,
        height: size.height,
        x: pos.as_ref().map(|p| p.x),
        y: pos.as_ref().map(|p| p.y),
        maximized: false,
    };
    state.is_worth_saving().then_some(state)
}

fn monitor_rects(w: &tauri::WebviewWindow) -> Vec<MonitorRect> {
    w.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            (p.x, p.y, s.width, s.height)
        })
        .collect()
}

/// Puts the main window back at its default size, centred on the primary monitor.
fn reset_window_geometry(w: &tauri::WebviewWindow) {
    let _ = w.unmaximize();
    let _ = w.set_size(PhysicalSize::new(
        window_state::DEFAULT_WIDTH,
        window_state::DEFAULT_HEIGHT,
    ));
    let _ = w.center();
}

/// Applies saved geometry only when it is usable and lands on a connected monitor.
fn restore_window_geometry(w: &tauri::WebviewWindow, saved: Option<WindowState>) {
    let Some(ws) = saved else {
        return;
    };
    let monitors = monitor_rects(w);
    if ws.size_is_usable() && ws.position_is_on_screen(&monitors) {
        let _ = w.set_size(PhysicalSize::new(ws.width, ws.height));
        if let (Some(x), Some(y)) = (ws.x, ws.y) {
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
    } else {
        tracing::warn!(?ws, "ignoring saved window geometry that is off-screen or too small");
        reset_window_geometry(w);
    }
    if ws.maximized {
        let _ = w.maximize();
    }
}

/// Shows, restores and focuses the main window, and pulls it back on-screen if it is not
/// on any connected monitor. Used by the tray, the single-instance handoff and startup.
pub(crate) fn bring_main_window_to_front(app: &tauri::AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.unminimize();
    let _ = w.show();
    if !w.is_maximized().unwrap_or(false) {
        let on_screen = match (w.outer_position(), w.inner_size()) {
            (Ok(p), Ok(s)) => WindowState {
                width: s.width,
                height: s.height,
                x: Some(p.x),
                y: Some(p.y),
                maximized: false,
            }
            .position_is_on_screen(&monitor_rects(&w)),
            _ => true,
        };
        if !on_screen {
            reset_window_geometry(&w);
        }
    }
    let _ = w.set_focus();
}

/// The app has no console, so a panic would otherwise vanish without a trace.
fn install_panic_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Jyroxa");
        let _ = std::fs::create_dir_all(&dir);
        let bt = std::backtrace::Backtrace::force_capture();
        let line = format!("{}\n{info}\n{bt}\n\n", chrono_now());
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("crash.log"))
        {
            let _ = f.write_all(line.as_bytes());
        }
        default_hook(info);
    }));
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix time {secs}")
}
fn format_speed(bps: f64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bps >= MIB {
        format!("{:.1} MB/s", bps / MIB)
    } else if bps >= 1024.0 {
        format!("{:.1} KB/s", bps / 1024.0)
    } else {
        format!("{:.0} B/s", bps)
    }
}

async fn run_tray_updater(app: tauri::AppHandle) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let Some(state) = app.try_state::<EngineState>() else {
            continue;
        };
        let settings = state.0.get_settings().await;
        let (active, pct, speed) = state.0.tray_snapshot().await;
        platform::sync_prevent_sleep(active, settings.prevent_sleep);

        let product = app.package_info().name.clone();
        let tooltip = if active > 0 {
            format!(
                "{product} - {active} active, {pct:.0}% - {}",
                format_speed(speed)
            )
        } else {
            product
        };
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let maybe_url = args
        .iter()
        .skip(1)
        .find(|a| a.starts_with("http://") || a.starts_with("https://"))
        .cloned();

    let _single_instance_guard =
        match platform::try_acquire_single_instance("JyroxaSingleInstanceMutex") {
            Some(g) => g,
            None => {
                platform::send_to_first_instance(maybe_url.as_deref());
                platform::focus_existing_window("jyroxa");
                return;
            }
        };

    install_panic_log();
    let start_minimized = args.iter().any(|a| a == "--minimized");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(platform::run_single_instance_server(handle.clone()));
            if let Some(url) = maybe_url.clone() {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                    let _ = h.emit("external-url", url);
                });
            }

            if let Some(w) = app.get_webview_window("main") {
                restore_window_geometry(&w, load_window_state(&handle));
            }

            tauri::async_runtime::block_on(async {
                init_engine(&handle).await.expect("engine init failed");
            });
            tauri::async_runtime::spawn(run_clipboard_monitor(handle.clone()));
            tauri::async_runtime::spawn(run_tray_updater(handle.clone()));

            let product = app.package_info().name.clone();
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
            let resume_i = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &pause_i, &resume_i, &quit_i])?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
                .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .menu(&menu)
                .tooltip(&product)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        bring_main_window_to_front(app);
                    }
                    "pause_all" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = app.try_state::<EngineState>() {
                                let _ = state.0.pause_all(app.clone()).await;
                            }
                        });
                    }
                    "resume_all" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = app.try_state::<EngineState>() {
                                let _ = state.0.resume_all(app.clone()).await;
                            }
                        });
                    }
                    "quit" => {
                        let app = app.clone();
                        tauri::async_runtime::block_on(async {
                            if let Some(state) = app.try_state::<EngineState>() {
                                let _ = state.0.save_queue_snapshot().await;
                            }
                        });
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        bring_main_window_to_front(tray.app_handle());
                    }
                })
                .build(app)?;

            if start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else {
                bring_main_window_to_front(&handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Some(ws) = snapshot_window(&app) {
                        save_window_state(&app, &ws);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    let close_to_tray = platform::close_to_tray();
                    if close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else if let Some(ws) = snapshot_window(&app) {
                        save_window_state(&app, &ws);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::list_downloads,
            commands::add_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::pause_many,
            commands::resume_many,
            commands::cancel_many,
            commands::remove_download,
            commands::remove_many,
            commands::retry_download,
            commands::grab_links,
            commands::probe_links,
            commands::probe_url,
            commands::suggest_save_path,
            commands::default_category_dirs,
            commands::open_folder_cmd,
            commands::open_file,
            commands::telegram_status,
            commands::telegram_set_credentials,
            commands::telegram_begin_login,
            commands::telegram_submit_code,
            commands::telegram_submit_password,
            commands::telegram_sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
