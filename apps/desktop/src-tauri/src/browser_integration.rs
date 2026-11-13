//! Browser extension native-messaging install + named-pipe handoff server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use jyroxa_native_host::handoff::{handoff_to_headers, BrowserHandoff, PipeEnvelope, PipeMessage};
use jyroxa_native_host::{NATIVE_HOST_NAME, HANDOFF_PIPE_NAME};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::engine::{AddDownloadArgs, BrowserIntegrationSettings, EngineState};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserIntegrationStatus {
    pub token_present: bool,
    pub host_manifest_present: bool,
    pub chrome_registry: bool,
    pub edge_registry: bool,
    pub firefox_registry: bool,
    pub native_host_installed: bool,
}

pub fn jyroxa_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Jyroxa")
}

fn token_path() -> PathBuf {
    jyroxa_data_dir().join("browser-integration.token")
}

fn desktop_exe_marker() -> PathBuf {
    jyroxa_data_dir().join("desktop-exe.txt")
}

fn host_manifest_path() -> PathBuf {
    jyroxa_data_dir()
        .join("native-host")
        .join(format!("{NATIVE_HOST_NAME}.json"))
}

pub fn ensure_browser_token() -> std::io::Result<String> {
    let path = token_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let t = existing.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let token = format!("{:x}{:x}{:x}", nanos, std::process::id(), line!());
    std::fs::write(&path, &token)?;
    Ok(token)
}

fn native_host_exe_path(desktop_exe: &Path) -> PathBuf {
    let sibling = desktop_exe
        .parent()
        .map(|p| p.join("jyroxa-native-host.exe"));
    if let Some(ref p) = sibling {
        if p.is_file() {
            return p.clone();
        }
    }
    jyroxa_data_dir().join("jyroxa-native-host.exe")
}

fn extension_origins(settings: &BrowserIntegrationSettings) -> Vec<String> {
    let mut out = Vec::new();
    if !settings.chrome_extension_id.is_empty() {
        out.push(format!(
            "chrome-extension://{}/",
            settings.chrome_extension_id.trim()
        ));
    }
    if !settings.edge_extension_id.is_empty() {
        out.push(format!(
            "chrome-extension://{}/",
            settings.edge_extension_id.trim()
        ));
    }
    if out.is_empty() {
        out.push("chrome-extension://lkcpmllaclbmggbphpaojdklcjgmjaad/".into());
    }
    out.push("moz-extension://jyroxa-browser@jyroxa/".into());
    out
}

#[cfg(windows)]
fn write_registry_manifest(key: &str, manifest: &Path) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (k, _) = hkcu.create_subkey(key).map_err(|e| e.to_string())?;
    k.set_value("", &manifest.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn write_registry_manifest(_key: &str, _manifest: &Path) -> Result<(), String> {
    Ok(())
}

pub fn install_browser_integration(
    desktop_exe: &Path,
    settings: &BrowserIntegrationSettings,
) -> Result<(), String> {
    let _token = ensure_browser_token().map_err(|e| e.to_string())?;
    let data = jyroxa_data_dir();
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    std::fs::write(desktop_exe_marker(), desktop_exe.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;

    let host_exe = native_host_exe_path(desktop_exe);
    if !host_exe.is_file() {
        return Err(format!(
            "native host not found at {}",
            host_exe.display()
        ));
    }

    let manifest_dir = data.join("native-host");
    std::fs::create_dir_all(&manifest_dir).map_err(|e| e.to_string())?;
    let manifest_path = host_manifest_path();

    let manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "Jyroxa browser download integration",
        "path": host_exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": extension_origins(settings),
    });
    let text =
        serde_json::to_string_pretty(&manifest).map_err(|e: serde_json::Error| e.to_string())?;
    std::fs::write(&manifest_path, text).map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        let m = manifest_path.to_string_lossy();
        let chrome_key = format!(r"Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}");
        let edge_key =
            format!(r"Software\Microsoft\Edge\NativeMessagingHosts\{NATIVE_HOST_NAME}");
        let firefox_key = format!(r"Software\Mozilla\NativeMessagingHosts\{NATIVE_HOST_NAME}");
        write_registry_manifest(&chrome_key, &manifest_path)?;
        write_registry_manifest(&edge_key, &manifest_path)?;
        write_registry_manifest(&firefox_key, &manifest_path)?;
        let _ = m;
    }
    Ok(())
}

pub fn uninstall_browser_integration() -> Result<(), String> {
    if token_path().is_file() {
        let _ = std::fs::remove_file(token_path());
    }
    if host_manifest_path().is_file() {
        let _ = std::fs::remove_file(host_manifest_path());
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for key in [
            format!(r"Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}"),
            format!(r"Software\Microsoft\Edge\NativeMessagingHosts\{NATIVE_HOST_NAME}"),
            format!(r"Software\Mozilla\NativeMessagingHosts\{NATIVE_HOST_NAME}"),
        ] {
            let _ = hkcu.delete_subkey(key);
        }
    }
    Ok(())
}

pub fn integration_status(desktop_exe: &Path) -> BrowserIntegrationStatus {
    let token_present = token_path().is_file();
    let host_manifest_present = host_manifest_path().is_file();
    let native_host_installed = native_host_exe_path(desktop_exe).is_file();

    #[cfg(windows)]
    fn reg_ok(key: &str) -> bool {
        use winreg::enums::*;
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(key)
            .is_ok()
    }

    #[cfg(windows)]
    let (chrome_registry, edge_registry, firefox_registry) = (
        reg_ok(&format!(
            r"Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}"
        )),
        reg_ok(&format!(
            r"Software\Microsoft\Edge\NativeMessagingHosts\{NATIVE_HOST_NAME}"
        )),
        reg_ok(&format!(
            r"Software\Mozilla\NativeMessagingHosts\{NATIVE_HOST_NAME}"
        )),
    );

    #[cfg(not(windows))]
    let (chrome_registry, edge_registry, firefox_registry) = (false, false, false);

    BrowserIntegrationStatus {
        token_present,
        host_manifest_present,
        chrome_registry,
        edge_registry,
        firefox_registry,
        native_host_installed,
    }
}

fn headers_map(h: &BrowserHandoff) -> HashMap<String, String> {
    handoff_to_headers(h)
        .into_iter()
        .collect::<HashMap<_, _>>()
}

async fn handle_pipe_message(app: &AppHandle, env: PipeEnvelope) -> serde_json::Value {
    let Ok(expected) = std::fs::read_to_string(token_path()) else {
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
            let _ = app.emit(
                "browser-link-grabber",
                serde_json::json!({ "html": html, "page_url": page_url }),
            );
            serde_json::json!({ "ok": true })
        }
        PipeMessage::Handoff(handoff) => {
            if let Err(e) = handoff.validate() {
                return serde_json::json!({ "ok": false, "error": e.to_string() });
            }
            let settings = state.0.get_settings().await;
            if !settings.browser_integration.enabled {
                return serde_json::json!({ "ok": false, "error": "browser integration disabled" });
            }

            let filename = handoff
                .filename
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "download".into());
            let headers = headers_map(&handoff);

            if settings.browser_integration.silent_add {
                let args = AddDownloadArgs {
                    urls: vec![handoff.url.clone()],
                    output_dir: settings.download_dir.clone(),
                    filename,
                    connections: settings.max_connections_per_download,
                    connections_auto: true,
                    sha256: None,
                    md5: None,
                    http_headers: Some(headers),
                };
                match state.0.add_download(app.clone(), args).await {
                    Ok(id) => serde_json::json!({ "ok": true, "download_id": id, "silent": true }),
                    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
                }
            } else {
                let payload = serde_json::json!({
                    "url": handoff.url,
                    "referrer": handoff.referrer,
                    "cookies": handoff.cookies,
                    "user_agent": handoff.user_agent,
                    "filename": handoff.filename,
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
