//! Browser extension native-messaging install + named-pipe handoff server.

use std::collections::HashMap;
use std::path::Path;

use jyroxa_native_host::handoff::{handoff_to_headers, BrowserHandoff, PipeEnvelope, PipeMessage};
use jyroxa_native_host::integration::{self, install_native_host, uninstall_native_host, BrowserOriginsConfig};
pub use jyroxa_native_host::integration::BrowserIntegrationStatus;
use jyroxa_native_host::HANDOFF_PIPE_NAME;
use tauri::{AppHandle, Emitter, Manager};

use crate::engine::{AddDownloadArgs, BrowserIntegrationSettings, EngineState};

pub use integration::ensure_browser_token;

fn origins_config(settings: &BrowserIntegrationSettings) -> BrowserOriginsConfig {
    BrowserOriginsConfig {
        chrome_extension_id: settings.chrome_extension_id.clone(),
        edge_extension_id: settings.edge_extension_id.clone(),
    }
}

pub fn install_browser_integration(
    desktop_exe: &Path,
    settings: &BrowserIntegrationSettings,
) -> Result<(), String> {
    install_native_host(desktop_exe, &origins_config(settings))
}

pub fn uninstall_browser_integration() -> Result<(), String> {
    uninstall_native_host()
}

pub fn ensure_browser_integration_on_start(
    desktop_exe: &Path,
    settings: &BrowserIntegrationSettings,
) {
    match install_native_host(desktop_exe, &origins_config(settings)) {
        Ok(()) => tracing::info!("browser native host registered"),
        Err(e) => tracing::warn!("browser native host registration skipped: {e}"),
    }
}

pub fn integration_status(desktop_exe: &Path) -> BrowserIntegrationStatus {
    integration::integration_status(desktop_exe)
}

fn headers_map(h: &BrowserHandoff) -> HashMap<String, String> {
    handoff_to_headers(h)
        .into_iter()
        .collect::<HashMap<_, _>>()
}

async fn handle_pipe_message(app: &AppHandle, env: PipeEnvelope) -> serde_json::Value {
    let Ok(expected) = std::fs::read_to_string(integration::jyroxa_data_dir().join("browser-integration.token")) else {
        return serde_json::json!({ "ok": false, "error": "token missing" });
    };
    if env.token.trim() != expected.trim() {
        return serde_json::json!({ "ok": false, "error": "invalid token" });
    }

    let Some(state) = app.try_state::<EngineState>() else {
        return serde_json::json!({ "ok": false, "error": "engine not ready" });
    };

    match env.message {
        PipeMessage::Ping => serde_json::json!({ "ok": true, "connected": true }),
        PipeMessage::GetSettings => {
            let settings = state.0.get_settings().await;
            let bi = settings.browser_integration.clone();
            serde_json::json!({
                "ok": true,
                "settings": bi,
            })
        }
        PipeMessage::GrabLinks { html, page_url } => {
            crate::bring_main_window_to_front(app);
            let _ = app.emit(
                "browser-link-grabber",
                serde_json::json!({ "html": html, "page_url": page_url }),
            );
            serde_json::json!({ "ok": true })
        }
        PipeMessage::SetSilentAdd { silent_add } => {
            let mut settings = state.0.get_settings().await;
            settings.browser_integration.silent_add = silent_add;
            let name = app.package_info().name.clone();
            let exe = std::env::current_exe().ok();
            match state
                .0
                .set_settings(settings, &name, exe.as_deref())
                .await
            {
                Ok(()) => serde_json::json!({ "ok": true, "silent_add": silent_add }),
                Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
            }
        }
        PipeMessage::Handoff(handoff) => {
            if let Err(e) = handoff.validate() {
                return serde_json::json!({ "ok": false, "error": e.to_string() });
            }
            let settings = state.0.get_settings().await;
            if !settings.browser_integration.enabled {
                return serde_json::json!({ "ok": false, "error": "browser integration disabled" });
            }

            let headers = headers_map(&handoff);
            let mut filename = handoff
                .filename
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "download".into());
            if let Ok(info) = state
                .0
                .probe_url_with_headers(&handoff.url, headers.clone())
                .await
            {
                if !info.suggested_name.is_empty() && info.suggested_name != "download" {
                    filename = info.suggested_name;
                }
            }

            if settings.browser_integration.silent_add {
                let args = AddDownloadArgs {
                    urls: vec![handoff.url.clone()],
                    output_dir: settings.download_dir.clone(),
                    filename: filename.clone(),
                    connections: settings.max_connections_per_download,
                    connections_auto: true,
                    sha256: None,
                    md5: None,
                    http_headers: Some(headers),
                    torrent_only_files: None,
                };
                match state.0.add_download(app.clone(), args).await {
                    Ok(id) => {
                        crate::bring_main_window_to_front(app);
                        crate::engine::show_native_notification(
                            app,
                            "Added to Jyroxa",
                            &filename,
                        );
                        serde_json::json!({ "ok": true, "download_id": id, "silent": true })
                    }
                    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
                }
            } else {
                crate::bring_main_window_to_front(app);
                let payload = serde_json::json!({
                    "url": handoff.url,
                    "referrer": handoff.referrer,
                    "cookies": handoff.cookies,
                    "user_agent": handoff.user_agent,
                    "filename": if filename == "download" { handoff.filename } else { Some(filename) },
                    "page_url": handoff.page_url,
                });
                let _ = app.emit("browser-handoff", payload);
                serde_json::json!({ "ok": true, "silent": false })
            }
        }
    }
}

#[cfg(windows)]
pub async fn run_browser_handoff_server(app: AppHandle) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;

    let pipe_name = HANDOFF_PIPE_NAME;
    let mut server = match ServerOptions::new()
        .first_pipe_instance(true)
        .create(pipe_name)
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("browser handoff pipe server failed: {e}");
            return;
        }
    };

    loop {
        if server.connect().await.is_ok() {
            let mut buf = vec![0u8; 64 * 1024];
            let n = server.read(&mut buf).await.unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]);
            let line = text.lines().next().unwrap_or("").trim();

            let response = if line.is_empty() {
                serde_json::json!({ "ok": false, "error": "empty" })
            } else if let Ok(env) = serde_json::from_str::<PipeEnvelope>(line) {
                handle_pipe_message(&app, env).await
            } else {
                serde_json::json!({ "ok": false, "error": "bad json" })
            };

            let out = format!("{}\n", serde_json::to_string(&response).unwrap_or_default());
            let _ = server.write_all(out.as_bytes()).await;
            let _ = server.flush().await;

            server = match ServerOptions::new().create(pipe_name) {
                Ok(s) => s,
                Err(_) => break,
            };
        }
    }
}

#[cfg(not(windows))]
pub async fn run_browser_handoff_server(_app: AppHandle) {}