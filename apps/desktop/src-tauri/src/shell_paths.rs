//! Windows Explorer path normalization and reveal-target resolution.

use std::path::PathBuf;

/// Strip `\\?\` / `\\?\UNC\` extended-path prefixes and normalize slashes for Explorer.
pub fn normalize_explorer_path(path: &str) -> String {
    let mut p = path.replace('/', "\\");
    if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        p = format!(r"\\{}", rest);
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        p = rest.to_string();
    }
    p
}

/// Decide what Explorer should open: select an existing file, or open a folder path.
pub fn resolve_explorer_target(
    raw_path: &str,
    download_dir_fallback: Option<&str>,
) -> Result<(bool, String), String> {
    let normalized_input = normalize_explorer_path(raw_path);
    let pb = PathBuf::from(&normalized_input);

    if pb.is_file() {
        return Ok((true, normalized_input));
    }
    if pb.is_dir() {
        return Ok((false, normalized_input));
    }

    // The file is gone (deleted, moved, or never written): open the folder it
    // lived in. `/select` on a missing file makes Explorer open Documents.
    if let Some(parent) = pb.parent().filter(|p| !p.as_os_str().is_empty()) {
        if parent.is_dir() {
            return Ok((false, normalize_explorer_path(&parent.to_string_lossy())));
        }
    }

    if let Some(dir) = download_dir_fallback.filter(|d| !d.trim().is_empty()) {
        let d = PathBuf::from(normalize_explorer_path(dir));
        if d.is_dir() {
            return Ok((false, normalize_explorer_path(&d.to_string_lossy())));
        }
    }

    Err(format!("Path not found: {raw_path}"))
}

#[cfg(windows)]
pub fn spawn_explorer_reveal(select_file: bool, explorer_path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let mut cmd = Command::new("explorer");
    if select_file {
        let arg = format!("/select,\"{explorer_path}\"");
        cmd.raw_arg(arg);
    } else {
        cmd.arg(explorer_path);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open Explorer: {e}"))
}

#[cfg(not(windows))]
pub fn spawn_explorer_reveal(select_file: bool, explorer_path: &str) -> Result<(), String> {
    let _ = select_file;
    std::process::Command::new("xdg-open")
        .arg(explorer_path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn reveal_path_in_shell(
    raw_path: &str,
    download_dir_fallback: Option<&str>,
) -> Result<(), String> {
    let (select_file, explorer_path) = resolve_explorer_target(raw_path, download_dir_fallback)?;
    spawn_explorer_reveal(select_file, &explorer_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(
            normalize_explorer_path(r"\\?\C:\Users\test\file.zip"),
            r"C:\Users\test\file.zip"
        );
    }

    #[test]
    fn strips_unc_prefix() {
        assert_eq!(
            normalize_explorer_path(r"\\?\UNC\server\share\file.txt"),
            r"\\server\share\file.txt"
        );
    }

    #[test]
    fn forward_slashes_to_backslashes() {
        assert_eq!(
            normalize_explorer_path("C:/Users/test/dir"),
            r"C:\Users\test\dir"
        );
    }

    #[test]
    fn existing_file_is_selected() {
        let exe = std::env::current_exe().unwrap();
        let (select, path) = resolve_explorer_target(&exe.to_string_lossy(), None).unwrap();
        assert!(select);
        assert_eq!(path, normalize_explorer_path(&exe.to_string_lossy()));
    }

    #[test]
    fn missing_file_opens_its_existing_parent() {
        let dir = std::env::temp_dir();
        let gone = dir.join("jyroxa-definitely-missing-file.bin");
        let (select, path) = resolve_explorer_target(&gone.to_string_lossy(), None).unwrap();
        assert!(!select);
        assert_eq!(path, normalize_explorer_path(&gone.parent().unwrap().to_string_lossy()));
    }

    #[test]
    fn nothing_exists_and_no_fallback_is_an_error() {
        assert!(resolve_explorer_target(r"Q:\no\such\file.dat", Some("")).is_err());
    }

    #[test]
    fn missing_file_uses_fallback_dir() {
        let (select, path) =
            resolve_explorer_target(r"C:\no\such\file.dat", Some(r"C:\Windows")).unwrap();
        assert!(!select);
        assert_eq!(path, r"C:\Windows");
    }
}
