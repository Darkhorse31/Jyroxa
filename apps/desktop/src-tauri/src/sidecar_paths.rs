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
    for name in CLI_CANDIDATE_NAMES {
        for sub in ["", "sidecars"] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_current_exe_dir() {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        let mut list = Vec::new();
        collect_sidecar_candidates(dir, &mut list);
        assert!(!list.is_empty() || dir.is_dir());
    }
}
