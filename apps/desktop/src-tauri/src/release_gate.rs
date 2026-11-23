//! Opt-in headless access to the real desktop engine for release verification.
//! No windows, tray, notifications, startup registration or browser registration.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::engine::{AddDownloadArgs, DesktopEngine, EngineState};

async fn command(
    app: &AppHandle,
    engine: &Arc<DesktopEngine>,
    input: Value,
) -> Result<Value, String> {
    let id = input.get("id").and_then(Value::as_u64).unwrap_or(0);
    match input["command"].as_str().unwrap_or("") {
        "list" => Ok(json!(engine.list_downloads().await)),
        "add" => {
            let args: AddDownloadArgs =
                serde_json::from_value(input["args"].clone()).map_err(|e| e.to_string())?;
            engine
                .add_download(app.clone(), args)
                .await
                .map(|id| json!(id))
                .map_err(|e| e.to_string())
        }
        "pause" => Ok(json!(engine.pause_and_mark(app.clone(), id).await)),
        "resume" => Ok(json!(engine.resume(app.clone(), id).await)),
        "retry" => Ok(json!(engine.retry_download(app.clone(), id).await)),
        "save" => engine
            .save_queue_snapshot()
            .await
            .map(|()| json!(true))
            .map_err(|e| e.to_string()),
        "restore" => engine
            .restore_queue(app.clone())
            .await
            .map(|()| json!(true))
            .map_err(|e| e.to_string()),
        "settings" => {
            let mut value =
                serde_json::to_value(engine.get_settings().await).map_err(|e| e.to_string())?;
            for (key, item) in input["settings"]
                .as_object()
                .ok_or("settings object required")?
            {
                value[key] = item.clone();
            }
            // Even an accidental settings request cannot display a toast.
            value["notifications"] = json!(false);
            let settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
            engine
                .set_settings(settings, "JyroxaReleaseGate", None)
                .await
                .map(|()| json!(true))
                .map_err(|e| e.to_string())
        }
        "video_formats" => {
            let url = input["url"].as_str().ok_or("url required")?;
            let info = crate::video_sidecar::probe_page_formats(url)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(crate::video_sidecar::formats_for_menu(&info)))
        }
        "quit" => {
            engine
                .save_queue_snapshot()
                .await
                .map_err(|e| e.to_string())?;
            app.exit(0);
            Ok(json!(true))
        }
        other => Err(format!("unknown gate command: {other}")),
    }
}

async fn serve(app: AppHandle, engine: Arc<DesktopEngine>) -> Result<(), String> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        let result = match serde_json::from_str(&line) {
            Ok(input) => command(&app, &engine, input).await,
            Err(e) => Err(e.to_string()),
        };
        let reply = match result {
            Ok(result) => json!({"ok": true, "result": result}),
            Err(error) => json!({"ok": false, "error": error}),
        };
        output
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        output.flush().await.map_err(|e| e.to_string())?;
    }
    engine
        .save_queue_snapshot()
        .await
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

pub fn run() {
    let data_dir = std::env::var_os("JYROXA_DATA_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .expect("set JYROXA_DATA_DIR to an isolated absolute directory");
    std::fs::create_dir_all(&data_dir).expect("create gate directory");
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .setup(move |app| {
            let engine = DesktopEngine::build(data_dir.clone())?;
            tauri::async_runtime::block_on(async {
                let mut settings = engine.get_settings().await;
                settings.notifications = false;
                engine
                    .set_settings(settings, "JyroxaReleaseGate", None)
                    .await
            })?;
            app.manage(EngineState(engine.clone()));
            crate::browser_integration::ensure_browser_token()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(crate::browser_integration::run_browser_handoff_server(
                handle.clone(),
            ));
            tauri::async_runtime::spawn(crate::engine::run_queue_watchdog(handle.clone()));
            tauri::async_runtime::spawn(async move {
                if let Err(error) = serve(handle.clone(), engine).await {
                    eprintln!("headless gate: {error}");
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .run(context)
        .expect("run headless release gate");
}
