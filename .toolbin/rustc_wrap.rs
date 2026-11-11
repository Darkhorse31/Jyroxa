// Cargo `build.rustc-wrapper`: inject an absolute `-Cdlltool=` for GNU Windows builds.
// See docs/BUILD_WINDOWS_GNU.md — rustc does not resolve `-Cdlltool` paths from the repo root.
use std::path::PathBuf;
use std::process::Command;
fn workspace_toolbin() -> Option<PathBuf> {
    if let Ok(wrap) = std::env::var("SWIFTGET_DLLTOOL_WRAP") {
        let path = PathBuf::from(wrap);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(ws) = std::env::var_os("CARGO_WORKSPACE_DIR") {
        roots.push(ws.into());
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR") {
        roots.push(manifest.into());
    }
    for start in roots {
        let mut dir = start.as_path();
        loop {
            let wrap = dir.join(".toolbin").join("dlltool-wrap.exe");
            if wrap.is_file() {
                return Some(wrap);
            }
            dir = dir.parent()?;
        }
    }
    None
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let rustc = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "rustc".to_string());

    let mut rustc_args: Vec<String> = args.drain(2..).collect();
    if let Some(wrap) = workspace_toolbin() {
        if !rustc_args.iter().any(|a| a.starts_with("-Cdlltool=")) {
            rustc_args.insert(
                0,
                format!("-Cdlltool={}", wrap.display()),
            );
            let winlibs = std::env::var_os("SWIFTGET_WINLIBS")
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
                .or_else(|| {
                    let p = wrap.parent()?.join("winlibs");
                    p.is_dir().then_some(p)
                });
            if let Some(winlibs) = winlibs {
                rustc_args.insert(0, format!("-Lnative={}", winlibs.display()));
            }
        }
    }

    let status = Command::new(&rustc)
        .args(&rustc_args)
        .status()
        .unwrap_or_else(|err| panic!("failed to run rustc {rustc}: {err:?}"));
    std::process::exit(status.code().unwrap_or(1));
}
