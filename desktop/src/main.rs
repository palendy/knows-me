//! knows-me desktop shell (Tauri 2).
//!
//! Thin GUI layer: it owns the window, manages a single [`AppState`], and exposes
//! the U1 command layer to the React frontend as `#[tauri::command]` handlers.
//! All real logic lives in the verified `knows-me-core` library — these wrappers
//! only translate `AppError` into a string for the IPC boundary.
//!
//! Build/run on a machine with the platform webview toolchain installed:
//! ```bash
//! npm install
//! npx tauri dev      # or: npx tauri build
//! ```

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use knows_me_core::core::commands::{self, AppStatus};
use knows_me_core::core::types::{AppConfig, TransferPolicy, TransferRecord};
use knows_me_core::AppState;
use tauri::Manager;

type CmdResult<T> = Result<T, String>;

fn err(e: knows_me_core::AppError) -> String {
    e.to_string()
}

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> AppStatus {
    commands::status(state.inner())
}

#[tauri::command]
async fn setup_password(state: tauri::State<'_, AppState>, password: String) -> CmdResult<()> {
    commands::setup_password(state.inner(), &password)
        .await
        .map_err(err)
}

#[tauri::command]
async fn unlock(state: tauri::State<'_, AppState>, password: String) -> CmdResult<()> {
    commands::unlock(state.inner(), &password).await.map_err(err)
}

#[tauri::command]
fn lock(state: tauri::State<'_, AppState>) {
    commands::lock(state.inner());
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
async fn set_transfer_policy(
    state: tauri::State<'_, AppState>,
    policy: TransferPolicy,
) -> CmdResult<()> {
    commands::set_transfer_policy(state.inner(), policy)
        .await
        .map_err(err)
}

#[tauri::command]
fn list_transfers(state: tauri::State<'_, AppState>) -> Vec<TransferRecord> {
    commands::list_transfers(state.inner())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Per-user encrypted data lives under the OS app-data directory.
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            setup_password,
            unlock,
            lock,
            get_config,
            set_transfer_policy,
            list_transfers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running knows-me");
}
