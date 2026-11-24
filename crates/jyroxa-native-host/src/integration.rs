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

/// Prefer the installed app over a development or temporary verification build.
pub fn installed_desktop_exe() -> Option<PathBuf> {
    if std::env::var_os("JYROXA_DATA_DIR").is_some() {
        return None;
    }
    let mut roots = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(&local).join("Programs/Jyroxa"));
        roots.push(PathBuf::from(local).join("Jyroxa"));
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            roots.push(PathBuf::from(root).join("Jyroxa"));
        }
    }
    roots
        .into_iter()
        .map(|p| p.join("jyroxa-desktop.exe"))
        .find(|p| p.is_file() && native_host_exe_beside_desktop(p).is_some())
}

pub fn host_manifest_path() -> PathBuf {
    jyroxa_data_dir()
        .join("native-host")
        .join(format!("{NATIVE_HOST_NAME}.json"))
}

pub fn firefox_manifest_path() -> PathBuf {
    host_manifest_path().with_file_name(format!("{NATIVE_HOST_NAME}.firefox.json"))
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
    desktop_exe.parent().and_then(|p| {
        [
            p.join("jyroxa-native-host.exe"),
            p.join("sidecars/jyroxa-native-host.exe"),
        ]
        .into_iter()
        .find(|p| p.is_file())
    })
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
        // Avoid replacing a running, already current host on every app startup.
        if std::fs::read(&source).ok() != std::fs::read(&dest).ok() {
            std::fs::copy(&source, &dest).map_err(|e| e.to_string())?;
        }
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

fn native_messaging_registry_keys() -> [String; 5] {
    [
        native_messaging_registry_key(r"Google\Chrome"),
        native_messaging_registry_key(r"Microsoft\Edge"),
        native_messaging_registry_key(r"BraveSoftware\Brave-Browser"),
        native_messaging_registry_key(r"Mozilla"),
        native_messaging_registry_key(r"Chromium"),
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

    ids.into_iter()
        .map(|id| format!("chrome-extension://{id}/"))
        .collect()
}

fn build_manifest(
    host_exe: &Path,
    config: &BrowserOriginsConfig,
    firefox: bool,
) -> serde_json::Value {
    let mut manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "Jyroxa browser download integration",
        "path": host_exe.to_string_lossy(),
        "type": "stdio",
    });
    if firefox {
        manifest["allowed_extensions"] = serde_json::json!(["jyroxa-browser@jyroxa"]);
    } else {
        manifest["allowed_origins"] = serde_json::json!(build_allowed_origins(config));
    }
    manifest
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
        local.join("Chromium").join("User Data"),
    ]
}

fn collect_ids_from_prefs_value(value: &serde_json::Value, profile: &Path, out: &mut Vec<String>) {
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
        // Modern Chromium stores unpacked extensions with only a path in Secure
        // Preferences. Inspect that manifest instead of relying on a cached name.
        let disk_manifest = ext.get("path").and_then(|p| p.as_str()).and_then(|p| {
            let path = PathBuf::from(p);
            let path = if path.is_absolute() {
                path
            } else {
                profile.join("Extensions").join(path)
            };
            std::fs::read_to_string(path.join("manifest.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        });
        if name == Some(want)
            || disk_manifest
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                == Some(want)
        {
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
                        collect_ids_from_prefs_value(&v, &path, &mut found);
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
    for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
        let (k, _) = hkcu
            .create_subkey_with_flags(key, KEY_WRITE | view)
            .map_err(|e| e.to_string())?;
        k.set_value("", &manifest.to_string_lossy().to_string())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_registry_manifest(_key: &str, _manifest: &Path) -> Result<(), String> {
    Ok(())
}

pub fn install_native_host(
    desktop_exe: &Path,
    config: &BrowserOriginsConfig,
) -> Result<(), String> {
    if !desktop_exe.is_absolute() || !desktop_exe.is_file() {
        return Err(format!(
            "Desktop executable does not exist: {}",
            desktop_exe.display()
        ));
    }
    // Validate sidecars before changing any working launch marker or manifest.
    if native_host_exe_beside_desktop(desktop_exe).is_none() {
        return Err(format!(
            "Native host is missing beside {}",
            desktop_exe.display()
        ));
    }
    let _token = ensure_browser_token().map_err(|e| e.to_string())?;
    let data = jyroxa_data_dir();
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let host_exe = sync_native_host_exe_to_data_dir(desktop_exe)?;
    std::fs::write(
        desktop_exe_marker(),
        desktop_exe.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let manifest_dir = data.join("native-host");
    std::fs::create_dir_all(&manifest_dir).map_err(|e| e.to_string())?;
    let manifest_path = host_manifest_path();

    let manifest = build_manifest(&host_exe, config, false);
    let text =
        serde_json::to_string_pretty(&manifest).map_err(|e: serde_json::Error| e.to_string())?;
    std::fs::write(&manifest_path, text).map_err(|e| e.to_string())?;
    std::fs::write(
        firefox_manifest_path(),
        serde_json::to_string_pretty(&build_manifest(&host_exe, config, true))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        for (index, key) in native_messaging_registry_keys().iter().enumerate() {
            let path = if index == 3 {
                firefox_manifest_path()
            } else {
                manifest_path.clone()
            };
            write_registry_manifest(key, &path)?;
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
    if firefox_manifest_path().is_file() {
        let _ = std::fs::remove_file(firefox_manifest_path());
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for key in native_messaging_registry_keys() {
            for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
                let _ = hkcu.delete_subkey_with_flags(&key, view);
            }
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
    pub chromium_registry: bool,
    pub native_host_installed: bool,
    pub packaged_extension_id: String,
}

pub fn integration_status(_desktop_exe: &Path) -> BrowserIntegrationStatus {
    let token_present = token_path().is_file();
    let host_manifest_present = host_manifest_path().is_file();
    let native_host_installed = std::fs::read_to_string(host_manifest_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|manifest| manifest["name"] == NATIVE_HOST_NAME && manifest["type"] == "stdio")
        .and_then(|manifest| manifest["path"].as_str().map(PathBuf::from))
        .is_some_and(|path| path.is_absolute() && path.is_file());

    #[cfg(windows)]
    fn reg_ok(key: &str, manifest: &Path) -> bool {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        [KEY_WOW64_32KEY, KEY_WOW64_64KEY].iter().all(|view| {
            hkcu.open_subkey_with_flags(key, KEY_READ | view)
                .ok()
                .and_then(|key| key.get_value::<String, _>("").ok())
                .is_some_and(|value| Path::new(&value) == manifest)
        })
    }

    #[cfg(windows)]
    let keys = native_messaging_registry_keys();
    #[cfg(windows)]
    let (chrome_registry, edge_registry, brave_registry, firefox_registry) = (
        reg_ok(&keys[0], &host_manifest_path()),
        reg_ok(&keys[1], &host_manifest_path()),
        reg_ok(&keys[2], &host_manifest_path()),
        reg_ok(&keys[3], &firefox_manifest_path()),
    );
    #[cfg(windows)]
    let chromium_registry = reg_ok(&keys[4], &host_manifest_path());
    #[cfg(not(windows))]
    let chromium_registry = false;

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
        chromium_registry,
        native_host_installed,
        packaged_extension_id: packaged_chrome_extension_id().to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
        assert!(origins.iter().all(|o| o.starts_with("chrome-extension://")));
        let firefox = build_manifest(
            Path::new("host.exe"),
            &BrowserOriginsConfig::default(),
            true,
        );
        assert_eq!(
            firefox["allowed_extensions"],
            serde_json::json!(["jyroxa-browser@jyroxa"])
        );
        assert!(firefox.get("allowed_origins").is_none());
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
        collect_ids_from_prefs_value(&sample, Path::new("."), &mut ids);
        assert_eq!(ids, vec!["hehgieooaeknhlodfpkommnhinaahkjf"]);
    }

    #[test]
    fn discovers_path_only_unpacked_and_relative_packaged_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        let unpacked = tmp.path().join("unpacked");
        let relative = tmp.path().join("Extensions/relative/1.0");
        for path in [&unpacked, &relative] {
            std::fs::create_dir_all(path).unwrap();
            std::fs::write(
                path.join("manifest.json"),
                r#"{"name":"Jyroxa Download Integration"}"#,
            )
            .unwrap();
        }
        let sample = serde_json::json!({"extensions":{"settings":{
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa":{"path":unpacked},
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb":{"path":"relative/1.0"},
            "invalid-id":{"manifest":{"name":"Jyroxa Download Integration"}},
            "cccccccccccccccccccccccccccccccc":{"path":"missing"},
            "dddddddddddddddddddddddddddddddd":{"manifest":{"name":"Other Extension"}}
        }}});
        let mut ids = Vec::new();
        collect_ids_from_prefs_value(&sample, tmp.path(), &mut ids);
        assert_eq!(
            ids,
            [
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ]
        );
    }

    #[test]
    fn incomplete_install_cannot_overwrite_a_working_launch_marker() {
        let _guard = MANIFEST_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("JYROXA_DATA_DIR", tmp.path());
        }
        std::fs::write(desktop_exe_marker(), "working-install").unwrap();
        let missing = tmp.path().join("missing.exe");
        assert!(install_native_host(&missing, &BrowserOriginsConfig::default()).is_err());
        std::fs::write(&missing, "desktop without host").unwrap();
        assert!(install_native_host(&missing, &BrowserOriginsConfig::default()).is_err());
        assert_eq!(
            std::fs::read_to_string(desktop_exe_marker()).unwrap(),
            "working-install"
        );
        assert!(!host_manifest_path().exists());
        unsafe {
            std::env::remove_var("JYROXA_DATA_DIR");
        }
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
        let _guard = MANIFEST_TEST_LOCK.lock().expect("manifest test lock");
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

    pub(crate) static MANIFEST_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
