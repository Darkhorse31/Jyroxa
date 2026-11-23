//! Native messaging host manifest + HKCU registry (Chrome, Edge, Firefox).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ext_id::{
    extension_display_name, packaged_chrome_extension_id, LEGACY_CHROME_EXTENSION_IDS,
    PACKAGED_CHROME_EXTENSION_ID,
};
use crate::NATIVE_HOST_NAME;

/// User-configured Chromium extension IDs (settings UI).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrowserOriginsConfig {
    pub chrome_extension_id: String,
    pub edge_extension_id: String,
}

pub fn jyroxa_data_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("JYROXA_DATA_DIR") {
        let p = PathBuf::from(override_dir);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
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

pub fn host_manifest_path() -> PathBuf {
    jyroxa_data_dir()
        .join("native-host")
        .join(format!("{NATIVE_HOST_NAME}.json"))
}

pub fn read_browser_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

fn native_host_exe_beside_desktop(desktop_exe: &Path) -> Option<PathBuf> {
    desktop_exe
        .parent()
        .map(|p| p.join("jyroxa-native-host.exe"))
        .filter(|p| p.is_file())
}

fn native_host_exe_path(desktop_exe: &Path) -> PathBuf {
    if let Some(p) = native_host_exe_beside_desktop(desktop_exe) {
        return p;
    }
    jyroxa_data_dir().join("jyroxa-native-host.exe")
}

/// Stable path under `%LOCALAPPDATA%\\Jyroxa` (Chrome reads this from the manifest).
fn sync_native_host_exe_to_data_dir(desktop_exe: &Path) -> Result<PathBuf, String> {
    let dest = jyroxa_data_dir().join("jyroxa-native-host.exe");
    let source = native_host_exe_beside_desktop(desktop_exe)
        .or_else(|| dest.is_file().then(|| dest.clone()))
        .ok_or_else(|| {
            format!(
                "native host not found beside {} or at {}",
                desktop_exe.display(),
                dest.display()
            )
        })?;
    if source != dest {
        std::fs::copy(&source, &dest).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

fn push_chrome_id(ids: &mut Vec<String>, raw: &str) {
    let id = raw.trim();
    if id.is_empty() || id.len() != 32 {
        return;
    }
    if id.chars().all(|c| ('a'..='p').contains(&c)) && !ids.iter().any(|x| x == id) {
        ids.push(id.to_string());
    }
}

/// When set (e.g. `JyroxaTest`), HKCU native-messaging keys are written under
/// `Software\<prefix>\...` so tests never touch real Chrome/Edge/Firefox keys.
fn native_messaging_registry_prefix() -> Option<String> {
    std::env::var("JYROXA_REGISTRY_PREFIX")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn native_messaging_registry_key(browser_vendor: &str) -> String {
    let leaf = format!(r"{browser_vendor}\NativeMessagingHosts\{NATIVE_HOST_NAME}");
    match native_messaging_registry_prefix() {
        Some(prefix) => format!(r"Software\{prefix}\{leaf}"),
        None => format!(r"Software\{leaf}"),
    }
}

fn native_messaging_registry_keys() -> [String; 4] {
    [
        native_messaging_registry_key(r"Google\Chrome"),
        native_messaging_registry_key(r"Microsoft\Edge"),
        native_messaging_registry_key(r"BraveSoftware\Brave-Browser"),
        native_messaging_registry_key(r"Mozilla"),
    ]
}

fn push_chrome_ids_from_existing_manifest(ids: &mut Vec<String>) {
    let path = host_manifest_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(arr) = v.get("allowed_origins").and_then(|a| a.as_array()) else {
        return;
    };
    for origin in arr {
        let Some(s) = origin.as_str() else {
            continue;
        };
        let Some(rest) = s.strip_prefix("chrome-extension://") else {
            continue;
        };
        let id = rest.strip_suffix('/').unwrap_or(rest);
        push_chrome_id(ids, id);
    }
}

/// Collect chrome-extension origins for the native host manifest.
pub fn build_allowed_origins(config: &BrowserOriginsConfig) -> Vec<String> {
    let mut ids = Vec::new();
    push_chrome_id(&mut ids, PACKAGED_CHROME_EXTENSION_ID);
    for legacy in LEGACY_CHROME_EXTENSION_IDS {
        push_chrome_id(&mut ids, legacy);
    }
    push_chrome_id(&mut ids, config.chrome_extension_id.trim());
    push_chrome_id(&mut ids, config.edge_extension_id.trim());
    for id in discover_chromium_extension_ids() {
        push_chrome_id(&mut ids, &id);
    }
    push_chrome_ids_from_existing_manifest(&mut ids);

    let mut out: Vec<String> = ids
        .into_iter()
        .map(|id| format!("chrome-extension://{id}/"))
        .collect();
    out.push("moz-extension://jyroxa-browser@jyroxa/".into());
    out
}

fn chromium_user_data_roots() -> Vec<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let Some(local) = local else {
        return Vec::new();
    };
    vec![
        local.join("Google").join("Chrome").join("User Data"),
        local.join("Microsoft").join("Edge").join("User Data"),
        local
            .join("BraveSoftware")
            .join("Brave-Browser")
            .join("User Data"),
    ]
}

fn collect_ids_from_prefs_value(value: &serde_json::Value, out: &mut Vec<String>) {
    let Some(settings) = value
        .pointer("/extensions/settings")
        .and_then(|v| v.as_object())
    else {
        return;
    };
    let want = extension_display_name();
    for (id, ext) in settings {
        let name = ext
            .pointer("/manifest/name")
            .or_else(|| ext.get("name"))
            .and_then(|n| n.as_str());
        if name == Some(want) {
            push_chrome_id(out, id);
        }
    }
}

/// Scan Chrome/Edge profiles for unpacked or sideloaded Jyroxa extensions.
pub fn discover_chromium_extension_ids() -> Vec<String> {
    let mut found = Vec::new();
    for root in chromium_user_data_roots() {
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            for file in ["Preferences", "Secure Preferences"] {
                let prefs = path.join(file);
                if let Ok(text) = std::fs::read_to_string(&prefs) {
                    if let Ok(v) = serde_json::from_str(&text) {
                        collect_ids_from_prefs_value(&v, &mut found);
                    }
                }
            }
        }
    }
    found
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

pub fn install_native_host(
    desktop_exe: &Path,
    config: &BrowserOriginsConfig,
) -> Result<(), String> {
    let _token = ensure_browser_token().map_err(|e| e.to_string())?;
    let data = jyroxa_data_dir();
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    std::fs::write(
        desktop_exe_marker(),
        desktop_exe.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let host_exe = sync_native_host_exe_to_data_dir(desktop_exe)?;

    let manifest_dir = data.join("native-host");
    std::fs::create_dir_all(&manifest_dir).map_err(|e| e.to_string())?;
    let manifest_path = host_manifest_path();

    let manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "Jyroxa browser download integration",
        "path": host_exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": build_allowed_origins(config),
    });
    let text =
        serde_json::to_string_pretty(&manifest).map_err(|e: serde_json::Error| e.to_string())?;
    std::fs::write(&manifest_path, text).map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        for key in native_messaging_registry_keys() {
            write_registry_manifest(&key, &manifest_path)?;
        }
    }
    Ok(())
}

pub fn uninstall_native_host() -> Result<(), String> {
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
        for key in native_messaging_registry_keys() {
            let _ = hkcu.delete_subkey(key);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserIntegrationStatus {
    pub token_present: bool,
    pub host_manifest_present: bool,
    pub chrome_registry: bool,
    pub edge_registry: bool,
    pub brave_registry: bool,
    pub firefox_registry: bool,
    pub native_host_installed: bool,
    pub packaged_extension_id: String,
}

pub fn integration_status(desktop_exe: &Path) -> BrowserIntegrationStatus {
    let token_present = token_path().is_file();
    let host_manifest_present = host_manifest_path().is_file();
    let native_host_installed = native_host_exe_path(desktop_exe).is_file();

    #[cfg(windows)]
    fn reg_ok(key: &str) -> bool {
        use winreg::enums::*;
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(key).is_ok()
    }

    #[cfg(windows)]
    let keys = native_messaging_registry_keys();
    #[cfg(windows)]
    let (chrome_registry, edge_registry, brave_registry, firefox_registry) = (
        reg_ok(&keys[0]),
        reg_ok(&keys[1]),
        reg_ok(&keys[2]),
        reg_ok(&keys[3]),
    );

    #[cfg(not(windows))]
    let (chrome_registry, edge_registry, brave_registry, firefox_registry) =
        (false, false, false, false);

    BrowserIntegrationStatus {
        token_present,
        host_manifest_present,
        chrome_registry,
        edge_registry,
        brave_registry,
        firefox_registry,
        native_host_installed,
        packaged_extension_id: packaged_chrome_extension_id().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext_id::PACKAGED_CHROME_EXTENSION_ID;

    #[test]
    fn allowed_origins_include_packaged_id() {
        let origins = build_allowed_origins(&BrowserOriginsConfig::default());
        assert!(origins
            .iter()
            .any(|o| o == &format!("chrome-extension://{PACKAGED_CHROME_EXTENSION_ID}/")));
        for legacy in LEGACY_CHROME_EXTENSION_IDS {
            assert!(origins
                .iter()
                .any(|o| o == &format!("chrome-extension://{legacy}/")));
        }
        assert!(origins
            .iter()
            .any(|o| o == "moz-extension://jyroxa-browser@jyroxa/"));
    }

    #[test]
    fn discover_ids_from_preferences_json() {
        let sample = serde_json::json!({
            "extensions": {
                "settings": {
                    "hehgieooaeknhlodfpkommnhinaahkjf": {
                        "manifest": { "name": "Jyroxa Download Integration" }
                    },
                    "abcdefghijklmnopabcdefghijklmnop": {
                        "manifest": { "name": "Other Extension" }
                    }
                }
            }
        });
        let mut ids = Vec::new();
        collect_ids_from_prefs_value(&sample, &mut ids);
        assert_eq!(ids, vec!["hehgieooaeknhlodfpkommnhinaahkjf"]);
    }

    #[test]
    fn manifest_json_shape() {
        use std::collections::HashSet;
        let origins = build_allowed_origins(&BrowserOriginsConfig {
            chrome_extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ..Default::default()
        });
        let unique: HashSet<_> = origins.iter().cloned().collect();
        assert_eq!(unique.len(), origins.len());
    }

    #[test]
    fn native_messaging_registry_key_paths() {
        // SAFETY: env mutation; other tests should not rely on JYROXA_REGISTRY_PREFIX.
        unsafe { std::env::remove_var("JYROXA_REGISTRY_PREFIX") };
        let prod = native_messaging_registry_key(r"Google\Chrome");
        assert_eq!(
            prod,
            format!(r"Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}")
        );
        unsafe { std::env::set_var("JYROXA_REGISTRY_PREFIX", "JyroxaTest") };
        let test_key = native_messaging_registry_key(r"Google\Chrome");
        assert!(test_key.contains(r"Software\JyroxaTest\Google\Chrome"));
        unsafe { std::env::remove_var("JYROXA_REGISTRY_PREFIX") };
    }

    static MANIFEST_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn preserves_ids_from_existing_manifest_on_disk() {
        let _guard = MANIFEST_TEST_LOCK.lock().expect("manifest test lock");
        let tmp = tempfile::tempdir().expect("tempdir");
        let data = tmp.path().join("jyroxa-data");
        std::fs::create_dir_all(data.join("native-host")).expect("dir");
        // SAFETY: isolated data dir for this test.
        unsafe { std::env::set_var("JYROXA_DATA_DIR", &data) };
        let manifest = data
            .join("native-host")
            .join(format!("{NATIVE_HOST_NAME}.json"));
        let prior = "hehgieooaeknhlodfpkommnhinaahkjf";
        let json = serde_json::json!({
            "name": NATIVE_HOST_NAME,
            "allowed_origins": [format!("chrome-extension://{prior}/")],
        });
        std::fs::write(
            &manifest,
            serde_json::to_string_pretty(&json).expect("json"),
        )
        .expect("write");
        let origins = build_allowed_origins(&BrowserOriginsConfig::default());
        assert!(origins
            .iter()
            .any(|o| o == &format!("chrome-extension://{prior}/")));
        assert!(origins
            .iter()
            .any(|o| o == &format!("chrome-extension://{PACKAGED_CHROME_EXTENSION_ID}/")));
        unsafe { std::env::remove_var("JYROXA_DATA_DIR") };
    }
}

#[cfg(all(windows, test))]
mod windows_live {
    use super::*;
    use crate::framing::{read_json_frame, write_json_frame};
    use crate::HANDOFF_PIPE_NAME;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn spawn_mock_desktop_pipe() -> Option<Arc<AtomicBool>> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in = stop.clone();
        let ready = Arc::new(AtomicBool::new(false));
        let ready_in = ready.clone();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .expect("rt");
            rt.block_on(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                use tokio::net::windows::named_pipe::ServerOptions;
                let mut server = None;
                for _ in 0..40 {
                    match ServerOptions::new()
                        .first_pipe_instance(true)
                        .create(HANDOFF_PIPE_NAME)
                    {
                        Ok(s) => {
                            server = Some(s);
                            ready_in.store(true, Ordering::Relaxed);
                            break;
                        }
                        Err(_) => thread::sleep(Duration::from_millis(50)),
                    }
                }
                let Some(mut server) = server else {
                    return;
                };
                while !stop_in.load(Ordering::Relaxed) {
                    if server.connect().await.is_err() {
                        continue;
                    }
                    let mut buf = vec![0u8; 64 * 1024];
                    let n = server.read(&mut buf).await.unwrap_or(0);
                    let text = String::from_utf8_lossy(&buf[..n]);
                    let line = text.lines().next().unwrap_or("").trim();
                    let resp = if line.contains("Ping") {
                        r#"{"ok":true,"connected":true}"#
                    } else {
                        r#"{"ok":false,"error":"unexpected"}"#
                    };
                    let out = format!("{resp}\n");
                    let _ = server.write_all(out.as_bytes()).await;
                    let _ = server.flush().await;
                    server = match ServerOptions::new().create(HANDOFF_PIPE_NAME) {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                }
            });
        });
        for _ in 0..40 {
            if ready.load(Ordering::Relaxed) {
                return Some(stop);
            }
            thread::sleep(Duration::from_millis(50));
        }
        None
    }

    #[test]
    fn install_and_framed_ping_in_temp_data_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data = tmp.path().join("jyroxa-data");
        std::fs::create_dir_all(&data).expect("data dir");
        // SAFETY: single-threaded test; env vars not read concurrently by other tests.
        unsafe {
            std::env::set_var("JYROXA_DATA_DIR", &data);
            std::env::set_var("JYROXA_REGISTRY_PREFIX", "JyroxaTest");
        };

        let stage = tmp.path().join("stage");
        std::fs::create_dir_all(&stage).expect("stage");
        let host_src = std::env::var_os("CARGO_BIN_EXE_jyroxa_native_host")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/debug/jyroxa-native-host.exe")
            });
        if !host_src.is_file() {
            eprintln!(
                "skip install_and_framed_ping: missing {}",
                host_src.display()
            );
            return;
        }
        let host_dst = stage.join("jyroxa-native-host.exe");
        std::fs::copy(&host_src, &host_dst).expect("copy host");
        let desktop = stage.join("jyroxa-desktop.exe");
        std::fs::write(&desktop, b"stub").expect("desktop stub");

        let Some(_pipe) = spawn_mock_desktop_pipe() else {
            eprintln!("skip install_and_framed_ping: pipe already in use (desktop running?)");
            return;
        };
        install_native_host(&desktop, &BrowserOriginsConfig::default()).expect("install");

        let mut child = Command::new(&host_dst)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("JYROXA_NATIVE_HOST_NO_LAUNCH", "1")
            .env("JYROXA_DATA_DIR", &data)
            .spawn()
            .expect("spawn host");
        {
            let stdin = child.stdin.as_mut().expect("stdin");
            write_json_frame(stdin, &serde_json::json!({ "type": "ping" })).expect("write ping");
        }
        let stdout = child.stdout.as_mut().expect("stdout");
        let reply = read_json_frame(stdout).expect("read reply");
        let _ = child.kill();
        let _ = child.wait();

        assert_eq!(reply.get("ok"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(reply.get("connected"), Some(&serde_json::Value::Bool(true)));

        uninstall_native_host().expect("cleanup test registry keys");
        unsafe {
            std::env::remove_var("JYROXA_DATA_DIR");
            std::env::remove_var("JYROXA_REGISTRY_PREFIX");
        };
    }
}
