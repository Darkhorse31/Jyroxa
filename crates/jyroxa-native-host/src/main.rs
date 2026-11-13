//! Chrome / Edge / Firefox native messaging host entry point.

use jyroxa_native_host::framing::{read_json_frame, write_json_frame, FramingError};
use jyroxa_native_host::handoff::{parse_extension_message, ExtensionMessage, PipeMessage};
use jyroxa_native_host::pipe_client::{forward_to_desktop, PipeError};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::Stdio;

fn token_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Jyroxa")
        .join("browser-integration.token")
}

fn read_install_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn respond<W: Write>(w: &mut W, value: serde_json::Value) {
    let _ = write_json_frame(w, &value);
}

fn try_launch_desktop() {
    if std::env::var("JYROXA_NATIVE_HOST_NO_LAUNCH").is_ok() {
        return;
    }
    let Some(appdata) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let marker = PathBuf::from(appdata).join("Jyroxa").join("desktop-exe.txt");
    let Ok(exe_text) = std::fs::read_to_string(&marker) else {
        return;
    };
    let exe = exe_text.trim();
    if exe.is_empty() {
        return;
    }
    let _ = std::process::Command::new(exe)
        .arg("--minimized")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn extension_to_pipe(msg: ExtensionMessage) -> PipeMessage {
    match msg {
        ExtensionMessage::Ping => PipeMessage::Ping,
        ExtensionMessage::GetSettings => PipeMessage::GetSettings,
        ExtensionMessage::GrabLinks { html, page_url } => PipeMessage::GrabLinks { html, page_url },
        ExtensionMessage::Handoff(h) | ExtensionMessage::DownloadUrl(h) => PipeMessage::Handoff(h),
    }
}

fn handle_message(
    _stdin: &mut impl Read,
    stdout: &mut impl Write,
    raw: serde_json::Value,
) -> Result<(), FramingError> {
    let parsed = match parse_extension_message(&raw) {
        Ok(m) => m,
        Err(e) => {
            respond(stdout, serde_json::json!({ "ok": false, "error": e.to_string() }));
            return Ok(());
        }
    };

    let Some(token) = read_install_token() else {
        respond(
            stdout,
            serde_json::json!({ "ok": false, "error": "native host not installed" }),
        );
        return Ok(());
    };

    let pipe_msg = extension_to_pipe(parsed);
    let mut result = forward_to_desktop(&token, pipe_msg.clone());

    if matches!(result, Err(PipeError::Unavailable(_))) {
        try_launch_desktop();
        std::thread::sleep(std::time::Duration::from_millis(800));
        result = forward_to_desktop(&token, pipe_msg);
    }

    match result {
        Ok(v) => respond(stdout, v),
        Err(e) => respond(stdout, serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
    Ok(())
}

fn main() {
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    loop {
        match read_json_frame(&mut stdin_lock) {
            Ok(v) => {
                if handle_message(&mut stdin_lock, &mut stdout_lock, v).is_err() {
                    break;
                }
            }
            Err(FramingError::Io(e))
                if e.kind() == io::ErrorKind::UnexpectedEof || e.kind() == io::ErrorKind::BrokenPipe =>
            {
                break;
            }
            Err(_) => break,
        }
    }
}
