#![cfg(windows)]

use jyroxa_native_host::framing::{read_json_frame, write_json_frame};
use jyroxa_native_host::handoff_pipe_name;
use jyroxa_native_host::integration::*;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn spawn_mock_desktop_pipe() -> Option<Arc<AtomicBool>> {
    let pipe_name = handoff_pipe_name();
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
                    .create(&pipe_name)
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
                let ping = serde_json::from_str::<jyroxa_native_host::handoff::PipeEnvelope>(line)
                    .is_ok_and(|envelope| {
                        matches!(
                            envelope.message,
                            jyroxa_native_host::handoff::PipeMessage::Ping
                        )
                    });
                let resp = if ping {
                    r#"{"ok":true,"connected":true}"#
                } else {
                    r#"{"ok":false,"error":"unexpected"}"#
                };
                let out = format!("{resp}\n");
                let _ = server.write_all(out.as_bytes()).await;
                let _ = server.flush().await;
                server = match ServerOptions::new().create(&pipe_name) {
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
    use std::os::windows::process::CommandExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let data = tmp.path().join("jyroxa-data");
    std::fs::create_dir_all(&data).expect("data dir");
    // SAFETY: single-threaded test; env vars not read concurrently by other tests.
    unsafe {
        std::env::set_var("JYROXA_DATA_DIR", &data);
        std::env::set_var(
            "JYROXA_REGISTRY_PREFIX",
            format!("JyroxaNativeTest{}", std::process::id()),
        );
        std::env::set_var(
            "JYROXA_HANDOFF_PIPE",
            format!(r"\\.\pipe\jyroxa-test-rust-{}", std::process::id()),
        );
    };

    let stage = tmp.path().join("stage");
    std::fs::create_dir_all(&stage).expect("stage");
    let host_src = PathBuf::from(env!("CARGO_BIN_EXE_jyroxa-native-host"));
    assert!(
        host_src.is_file(),
        "native host must be built for this test"
    );
    let host_dst = stage.join("jyroxa-native-host.exe");
    std::fs::copy(&host_src, &host_dst).expect("copy host");
    let desktop = stage.join("jyroxa-desktop.exe");
    std::fs::write(&desktop, b"stub").expect("desktop stub");

    let _pipe = spawn_mock_desktop_pipe().expect("isolated mock pipe must start");
    install_native_host(&desktop, &BrowserOriginsConfig::default()).expect("install");
    let status = integration_status(&desktop);
    assert!(status.native_host_installed && status.host_manifest_present && status.token_present);
    assert!(
        status.chrome_registry
            && status.edge_registry
            && status.brave_registry
            && status.chromium_registry
            && status.firefox_registry
    );

    let mut child = Command::new(&host_dst)
        .creation_flags(0x0800_0000)
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
    _pipe.store(true, Ordering::Relaxed);

    uninstall_native_host().expect("cleanup test registry keys");
    unsafe {
        std::env::remove_var("JYROXA_DATA_DIR");
        std::env::remove_var("JYROXA_REGISTRY_PREFIX");
        std::env::remove_var("JYROXA_HANDOFF_PIPE");
    };
}
