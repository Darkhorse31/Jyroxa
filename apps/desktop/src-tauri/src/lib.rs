mod commands;
mod engine;
mod platform;

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use engine::{init_engine, run_clipboard_monitor, EngineState};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WindowState {
    width: u32,
    height: u32,
    x: Option<i32>,
    y: Option<i32>,
}

fn window_state_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("swiftget").join("window.json"))
}

fn load_window_state(app: &tauri::AppHandle) -> Option<WindowState> {
    let path = window_state_path(app)?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_window_state(app: &tauri::AppHandle, state: &WindowState) {
    let Some(path) = window_state_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, text);
    }
}

fn snapshot_window(app: &tauri::AppHandle) -> Option<WindowState> {
    let w = app.get_webview_window("main")?;
    let size = w.inner_size().ok()?;
    let pos = w.outer_position().ok();
    Some(WindowState {
        width: size.width,
        height: size.height,
        x: pos.as_ref().map(|p| p.x),
        y: pos.as_ref().map(|p| p.y),
    })
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

    let _single_instance_guard = match platform::try_acquire_single_instance("SwiftGetSingleInstanceMutex") {
        Some(g) => g,
        None => {
            platform::send_to_first_instance(maybe_url.as_deref());
            platform::focus_existing_window("SwiftGet");
            return;
        }
    };

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

            if let Some(ws) = load_window_state(&handle) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_size(PhysicalSize::new(ws.width, ws.height));
                    if let (Some(x), Some(y)) = (ws.x, ws.y) {
                        let _ = w.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            tauri::async_runtime::block_on(async {
                init_engine(&handle).await.expect("engine init failed");
            });
            tauri::async_runtime::spawn(run_clipboard_monitor(handle.clone()));
            tauri::async_runtime::spawn(run_tray_updater(handle.clone()));

            let product = app.package_info().name.clone();
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
            let resume_i =
                MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
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
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
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
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            if start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
