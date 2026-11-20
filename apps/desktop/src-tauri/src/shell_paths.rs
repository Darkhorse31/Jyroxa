//! Windows Explorer path normalization and reveal-target resolution.

use std::path::{Path, PathBuf};

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

fn path_exists(p: &Path) -> bool {
    p.is_file() || p.is_dir()
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

    if let Some(parent) = pb.parent() {
        if path_exists(parent) {
            if pb.file_name().is_some() {
                return Ok((true, normalized_input));
            }
            return Ok((false, normalize_explorer_path(&parent.to_string_lossy())));
        }
    }

    if let Some(dir) = download_dir_fallback {
        let d = PathBuf::from(normalize_explorer_path(dir));
        if path_exists(&d) {
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
    fn missing_file_uses_fallback_dir() {
        let (select, path) =
            resolve_explorer_target(r"C:\no\such\file.dat", Some(r"C:\Windows")).unwrap();
        assert!(!select);
        assert_eq!(path, r"C:\Windows");
    }
}
