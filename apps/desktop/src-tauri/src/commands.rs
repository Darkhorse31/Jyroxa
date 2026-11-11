use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::engine::{AddDownloadArgs, AppSettings, DownloadView, EngineState, GrabOptionsDto};
use crate::platform::{open_file_in_folder, open_folder};
use swiftget_core::linkgrabber::{LinkInfo, Package};
use swiftget_core::ProbeInfo;

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
pub fn open_folder_cmd(path: String) -> Result<(), String> {
    open_folder(PathBuf::from(path).as_path())
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    open_file_in_folder(PathBuf::from(path).as_path())
}
