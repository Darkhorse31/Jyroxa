use tauri::{AppHandle, State};

use crate::engine::{
    AddDownloadArgs, AppSettings, DownloadView, EngineState, GrabOptionsDto,
};
use swiftget_core::linkgrabber::{LinkInfo, Package};

#[tauri::command]
pub async fn get_settings(state: State<'_, EngineState>) -> Result<AppSettings, String> {
    Ok(state.0.get_settings().await)
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, EngineState>,
    settings: AppSettings,
) -> Result<(), String> {
    state
        .0
        .set_settings(settings)
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
