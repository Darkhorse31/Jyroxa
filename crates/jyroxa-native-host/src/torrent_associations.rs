//! HKCU registration for `magnet:` and `.torrent` → jyroxa-desktop.exe (no admin).

use std::path::Path;

const TORRENT_PROG_ID: &str = "Jyroxa.torrent";

/// When set, association keys live under `Software\<prefix>\Classes\...` for tests.
fn registry_prefix() -> Option<String> {
    std::env::var("JYROXA_REGISTRY_PREFIX")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn classes_root() -> String {
    match registry_prefix() {
        Some(prefix) => format!(r"Software\{prefix}\Classes"),
        None => r"Software\Classes".to_string(),
    }
}

#[cfg(windows)]
fn set_default(hkcu: &winreg::RegKey, key: &str, value: &str) -> Result<(), String> {
    let (k, _) = hkcu.create_subkey(key).map_err(|e| e.to_string())?;
    k.set_value("", &value).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn delete_tree(hkcu: &winreg::RegKey, key: &str) -> Result<(), String> {
    hkcu.delete_subkey_all(key).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn install_torrent_associations(desktop_exe: &Path) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let root = classes_root();
    let exe = desktop_exe.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");

    // magnet: URL protocol
    let magnet = format!(r"{root}\magnet");
    set_default(&hkcu, &magnet, "URL:Magnet Protocol")?;
    set_default(&hkcu, &format!(r"{magnet}\shell\open\command"), &open_cmd)?;
    let (k, _) = hkcu.create_subkey(&magnet).map_err(|e| e.to_string())?;
    k.set_value("URL Protocol", &"")
        .map_err(|e| e.to_string())?;

    // .torrent → Jyroxa.torrent → open command
    set_default(&hkcu, &format!(r"{root}\.torrent"), TORRENT_PROG_ID)?;
    set_default(
        &hkcu,
        &format!(r"{root}\{TORRENT_PROG_ID}"),
        "BitTorrent document",
    )?;
    set_default(
        &hkcu,
        &format!(r"{root}\{TORRENT_PROG_ID}\shell\open\command"),
        &open_cmd,
    )?;

    Ok(())
}

#[cfg(not(windows))]
pub fn install_torrent_associations(_desktop_exe: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn uninstall_torrent_associations() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let root = classes_root();
    let _ = delete_tree(&hkcu, &format!(r"{root}\magnet"));
    let _ = delete_tree(&hkcu, &format!(r"{root}\{TORRENT_PROG_ID}"));
    let (ext_key, _) = hkcu
        .create_subkey(format!(r"{root}\.torrent"))
        .map_err(|e| e.to_string())?;
    let _ = ext_key.delete_value("");
    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall_torrent_associations() -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn torrent_assoc_install_under_test_prefix() {
        let _g = crate::integration::tests::MANIFEST_TEST_LOCK
            .lock()
            .expect("registry test lock");
        let prefix = format!("JyroxaTestTorrentAssoc_{}", std::process::id());
        std::env::set_var("JYROXA_REGISTRY_PREFIX", &prefix);
        let desktop = std::env::temp_dir().join("jyroxa-desktop-test.exe");
        install_torrent_associations(&desktop).expect("install");
        uninstall_torrent_associations().expect("uninstall");
        std::env::remove_var("JYROXA_REGISTRY_PREFIX");
    }
}
