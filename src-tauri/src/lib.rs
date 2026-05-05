mod keeper;
mod scan;
mod thumb;

use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Photo {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ClusterKind {
    Exact,
    Similar,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Cluster {
    pub id: String,
    pub kind: ClusterKind,
    pub photos: Vec<Photo>,
    pub keeper_index: usize,
    pub reclaimable_bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScanResult {
    pub exact: Vec<Cluster>,
    pub similar: Vec<Cluster>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TrashResult {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[tauri::command]
async fn scan_directory(
    app: AppHandle,
    path: String,
    max_difference: u32,
) -> Result<ScanResult, String> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }

    let app_for_progress = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        scan::run_scan(&root, max_difference, move |ev| {
            let _ = app_for_progress.emit("scan-progress", ev);
        })
    })
    .await
    .map_err(|e| format!("scan task panicked: {e}"))??;

    let _ = app.emit("scan-complete", &result);
    Ok(result)
}

#[tauri::command]
async fn get_thumbnail(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || thumb::thumbnail_data_url(Path::new(&path)))
        .await
        .map_err(|e| format!("thumbnail task panicked: {e}"))?
}

#[tauri::command]
async fn move_to_trash(paths: Vec<String>) -> Vec<TrashResult> {
    tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| match trash::delete(&p) {
                Ok(_) => TrashResult { path: p, ok: true, error: None },
                Err(e) => TrashResult {
                    path: p,
                    ok: false,
                    error: Some(format!("{e}")),
                },
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // czkawka_core needs its config + cache directories set once before any scan.
    // On macOS this resolves to ~/Library/Application Support/pl.Qarmin.doppelganger/.
    let _ = czkawka_core::common::config_cache_path::set_config_cache_path("doppelganger", "doppelganger");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_directory,
            get_thumbnail,
            move_to_trash
        ])
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                if let Some(window) = _app.get_webview_window("main") {
                    window.open_devtools();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
