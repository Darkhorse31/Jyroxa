use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::engine::{AddDownloadArgs, AppSettings, DownloadView, EngineState, GrabOptionsDto};
use crate::platform::reveal_in_file_manager;
use tauri_plugin_opener::OpenerExt;
use jyroxa_core::linkgrabber::{LinkInfo, Package};
use jyroxa_core::ProbeInfo;

#[tauri::command]
pub async fn get_settings(state: State<'_, EngineState>) -> Result<AppSettings, String> {
    Ok(state.0.get_settings().await)
}

#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, EngineState>,
    settings: AppSettings,
) -> Result<(), String> {
    let name = app.package_info().name.clone();
    let exe = std::env::current_exe().ok();
    state
        .0
        .set_settings(settings, &name, exe.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, EngineState>) -> Result<Vec<DownloadView>, String> {
    Ok(state.0.list_downloads().await)
}

#[tauri::command]
pub async fn add_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    args: AddDownloadArgs,
) -> Result<u64, String> {
    state
        .0
        .add_download(app, args)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    id: u64,
) -> Result<bool, String> {
    Ok(state.0.pause_and_mark(app, id).await)
}

#[tauri::command]
pub async fn resume_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    id: u64,
) -> Result<bool, String> {
    Ok(state.0.resume(app, id).await)
}

#[tauri::command]
pub async fn cancel_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    id: u64,
) -> Result<bool, String> {
    Ok(state.0.cancel(app, id).await)
}

#[tauri::command]
pub async fn pause_many(
    app: AppHandle,
    state: State<'_, EngineState>,
    ids: Vec<u64>,
) -> Result<usize, String> {
    Ok(state.0.pause_many(app, &ids).await)
}

#[tauri::command]
pub async fn resume_many(
    app: AppHandle,
    state: State<'_, EngineState>,
    ids: Vec<u64>,
) -> Result<usize, String> {
    Ok(state.0.resume_many(app, &ids).await)
}

#[tauri::command]
pub async fn cancel_many(
    app: AppHandle,
    state: State<'_, EngineState>,
    ids: Vec<u64>,
) -> Result<usize, String> {
    Ok(state.0.cancel_many(app, &ids).await)
}

#[tauri::command]
pub async fn remove_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    id: u64,
    delete_file: bool,
) -> Result<bool, String> {
    Ok(state.0.remove_download(app, id, delete_file).await)
}

#[tauri::command]
pub async fn remove_many(
    app: AppHandle,
    state: State<'_, EngineState>,
    ids: Vec<u64>,
    delete_file: bool,
) -> Result<usize, String> {
    Ok(state.0.remove_many(app, &ids, delete_file).await)
}

#[tauri::command]
pub async fn retry_download(
    app: AppHandle,
    state: State<'_, EngineState>,
    id: u64,
) -> Result<bool, String> {
    Ok(state.0.retry_download(app, id).await)
}

#[tauri::command]
pub async fn grab_links(
    state: State<'_, EngineState>,
    input: String,
    options: GrabOptionsDto,
) -> Result<Vec<Package>, String> {
    state
        .0
        .grab_links(&input, &options)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn probe_links(
    state: State<'_, EngineState>,
    urls: Vec<String>,
) -> Result<Vec<LinkInfo>, String> {
    Ok(state.0.probe_urls(urls).await)
}

#[tauri::command]
pub async fn probe_url(state: State<'_, EngineState>, url: String) -> Result<ProbeInfo, String> {
    state.0.probe_url(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn suggest_save_path(state: State<'_, EngineState>, dir: String, name: String) -> String {
    state.0.suggest_save_path(&dir, &name)
}

#[tauri::command]
pub async fn default_category_dirs(
    app: AppHandle,
    state: State<'_, EngineState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let label = app.package_info().name.clone();
    Ok(state.0.default_category_dirs(&label).await)
}

#[tauri::command]
pub async fn open_folder_cmd(
    state: State<'_, EngineState>,
    path: String,
) -> Result<(), String> {
    let settings = state.0.get_settings().await;
    let fallback = settings.download_dir;
    reveal_in_file_manager(PathBuf::from(&path).as_path(), Some(fallback.as_str()))
}

#[tauri::command]
pub fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    let pb = PathBuf::from(&path);
    if !pb.is_file() {
        return Err(format!("File not found: {path}"));
    }
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("Could not open file: {e}"))
}

#[cfg(test)]
mod command_arg_tests {
    use crate::engine::{AddDownloadArgs, GrabOptionsDto};
    use serde_json::json;

    fn snake_from_camel(key: &str) -> String {
        let mut out = String::new();
        for (i, ch) in key.chars().enumerate() {
            if ch.is_uppercase() {
                if i > 0 {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn remove_many_frontend_payload() {
        let payload = json!({ "ids": [1u64, 2], "deleteFile": true });
        let delete_file: bool = serde_json::from_value(payload["deleteFile"].clone()).unwrap();
        let ids: Vec<u64> = serde_json::from_value(payload["ids"].clone()).unwrap();
        assert!(delete_file);
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn add_download_args_from_frontend() {
        let args_val = json!({
            "urls": ["https://proof.ovh.net/files/10Mb.dat"],
            "output_dir": "C:\\Downloads",
            "filename": "10Mb.dat",
            "connections": 8,
            "connections_auto": true
        });
        let args: AddDownloadArgs = serde_json::from_value(args_val).unwrap();
        assert_eq!(args.urls.len(), 1);
        assert!(args.connections_auto);
    }

    #[test]
    fn grab_links_options_from_frontend() {
        let options_val = json!({
            "deep": true,
            "no_probe": false,
            "include_ext": ["zip", "dat"],
            "host_filter": "example.com"
        });
        let options: GrabOptionsDto = serde_json::from_value(options_val).unwrap();
        assert!(options.deep);
        assert!(!options.no_probe);
        assert_eq!(options.include_ext.as_ref().map(|v| v.len()), Some(2));
    }

    #[test]
    fn camel_case_mapping_sample() {
        assert_eq!(snake_from_camel("deleteFile"), "delete_file");
        assert_eq!(snake_from_camel("outputDir"), "output_dir");
    }
}
