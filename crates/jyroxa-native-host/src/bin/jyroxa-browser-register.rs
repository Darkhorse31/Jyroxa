//! Install/uninstall native messaging host (NSIS hooks + manual/CI verification).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use jyroxa_native_host::framing::{read_json_frame, write_json_frame};
use jyroxa_native_host::integration::{
    install_native_host, uninstall_native_host, BrowserOriginsConfig,
};
use jyroxa_native_host::torrent_associations::{
    install_torrent_associations, uninstall_torrent_associations,
};

fn usage() -> ! {
    eprintln!(
        "usage:\n  jyroxa-browser-register install <jyroxa-desktop.exe>\n  jyroxa-browser-register uninstall\n  jyroxa-browser-register verify <jyroxa-desktop.exe>"
    );
    std::process::exit(2);
}

fn host_exe_beside(desktop: &PathBuf) -> PathBuf {
    desktop
        .parent()
        .map(|p| p.join("jyroxa-native-host.exe"))
        .unwrap_or_else(|| PathBuf::from("jyroxa-native-host.exe"))
}

#[cfg(windows)]
fn framed_ping(host_exe: &PathBuf) -> Result<serde_json::Value, String> {
    let mut child = Command::new(host_exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("JYROXA_NATIVE_HOST_NO_LAUNCH", "1")
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdin = child.stdin.as_mut().ok_or("stdin")?;
    let stdout = child.stdout.as_mut().ok_or("stdout")?;
    write_json_frame(stdin, &serde_json::json!({ "type": "ping" })).map_err(|e| e.to_string())?;
    let reply = read_json_frame(stdout).map_err(|e| e.to_string())?;
    let _ = child.kill();
    let _ = child.wait();
    Ok(reply)
}

#[cfg(not(windows))]
fn framed_ping(_host_exe: &PathBuf) -> Result<serde_json::Value, String> {
    Err("framed ping only supported on Windows".into())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("install") => {
            let desktop = args.get(2).map(PathBuf::from).unwrap_or_else(|| usage());
            let cfg = BrowserOriginsConfig::default();
            if let Err(e) = install_native_host(&desktop, &cfg) {
                eprintln!("install failed: {e}");
                std::process::exit(1);
            }
            if let Err(e) = install_torrent_associations(&desktop) {
                eprintln!("torrent association install failed: {e}");
                std::process::exit(1);
            }
        }
        Some("uninstall") => {
            if let Err(e) = uninstall_torrent_associations() {
                eprintln!("torrent association uninstall failed: {e}");
                std::process::exit(1);
            }
            if let Err(e) = uninstall_native_host() {
                eprintln!("uninstall failed: {e}");
                std::process::exit(1);
            }
        }
        Some("verify") => {
            let desktop = args.get(2).map(PathBuf::from).unwrap_or_else(|| usage());
            let cfg = BrowserOriginsConfig::default();
            install_native_host(&desktop, &cfg).unwrap_or_else(|e| {
                eprintln!("install failed: {e}");
                std::process::exit(1);
            });
            let host = host_exe_beside(&desktop);
            let reply = framed_ping(&host).unwrap_or_else(|e| {
                eprintln!("ping failed: {e}");
                std::process::exit(1);
            });
            if reply.get("ok") != Some(&serde_json::Value::Bool(true)) {
                eprintln!("unexpected reply: {reply}");
                std::process::exit(1);
            }
            println!("verify ok");
        }
        _ => usage(),
    }
}