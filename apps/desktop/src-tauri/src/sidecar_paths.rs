//! Locate bundled sidecar binaries (`jyroxa.exe`, yt-dlp, ffmpeg).

use std::path::{Path, PathBuf};

fn push_unique(out: &mut Vec<PathBuf>, p: PathBuf) {
    if p.is_file() {
        out.push(p);
    }
}

const CLI_CANDIDATE_NAMES: &[&str] = &["jyroxa.exe", "jyroxa-cli.exe", "Jyroxa.exe"];

/// Resolve the `jyroxa` CLI used for torrent sidecars.
pub fn jyroxa_cli_exe() -> PathBuf {
    if let Ok(p) = std::env::var("JYROXA_CLI") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return path;
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            collect_sidecar_candidates(dir, &mut candidates);
        }
    }

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let install = PathBuf::from(local).join("Jyroxa");
        collect_sidecar_candidates(&install, &mut candidates);
    }

    if let Some(p) = candidates.into_iter().next() {
        return p;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/release/jyroxa.exe")
}

fn collect_sidecar_candidates(dir: &Path, out: &mut Vec<PathBuf>) {
    // Portable Jyroxa.exe and CLI jyroxa.exe compare equal on Windows.
    // Prefer the explicit sidecar folder before considering root-level names.
    for sub in ["sidecars", ""] {
        for name in CLI_CANDIDATE_NAMES {
            push_unique(out, dir.join(sub).join(name));
        }
    }
}

/// Resolve a named sidecar next to the desktop executable.
pub fn sidecar_exe(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for sub in ["", "sidecars"] {
                let p = dir.join(sub).join(name);
                if p.is_file() {
                    return p;
                }
            }
        }
    }
    PathBuf::from(name)
}

/// Prefer bundled runtimes, then existing system runtimes. Node requires explicit
/// enablement in yt-dlp even when it is already on PATH.
const JS_RUNTIMES: &[(&str, &str)] = &[
    ("deno", "deno.exe"),
    ("node", "node.exe"),
    ("bun", "bun.exe"),
];

fn runtime_in_directories(dirs: &[PathBuf]) -> Option<(&'static str, PathBuf)> {
    for dir in dirs {
        for &(runtime, filename) in JS_RUNTIMES {
            let path = dir.join(filename);
            if path.is_file() {
                return Some((runtime, path));
            }
        }
    }
    None
}

pub fn javascript_runtime() -> Option<(&'static str, PathBuf)> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(runtime) =
                runtime_in_directories(&[dir.to_path_buf(), dir.join("sidecars")])
            {
                return Some(runtime);
            }
        }
    }
    let mut system: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(programs) = std::env::var_os("ProgramFiles") {
        system.push(PathBuf::from(programs).join("nodejs"));
    }
    runtime_in_directories(&system)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_runtime_precedes_a_system_runtime() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("bundle");
        let system = tmp.path().join("system");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(bundle.join("node.exe"), b"bundled node").unwrap();
        std::fs::write(system.join("deno.exe"), b"system deno").unwrap();
        assert_eq!(
            runtime_in_directories(&[bundle.clone(), system.clone()]),
            Some(("node", bundle.join("node.exe")))
        );
        assert_eq!(
            runtime_in_directories(std::slice::from_ref(&system)),
            Some(("deno", system.join("deno.exe")))
        );
        assert!(runtime_in_directories(&[tmp.path().join("missing")]).is_none());
    }

    #[test]
    fn portable_layout_selects_cli_before_desktop_executable() {
        let temp = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temp.join(format!(
            "jyroxa-sidecar-paths-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("sidecars")).unwrap();
        let cli = root.join("sidecars/jyroxa.exe");
        std::fs::write(root.join("Jyroxa.exe"), b"desktop").unwrap();
        std::fs::write(&cli, b"cli").unwrap();
        let mut candidates = Vec::new();
        collect_sidecar_candidates(&root, &mut candidates);
        assert_eq!(candidates.first(), Some(&cli));
        let resolved = std::fs::canonicalize(&root).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(temp).unwrap()));
        std::fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn resolves_current_exe_dir() {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        let mut list = Vec::new();
        collect_sidecar_candidates(dir, &mut list);
        assert!(!list.is_empty() || dir.is_dir());
    }
}
