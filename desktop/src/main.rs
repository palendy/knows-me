//! knows-me desktop shell (Tauri 2).
//!
//! Thin GUI layer: it owns the window, manages a single [`AppState`] plus a
//! [`Services`] registry for the unlocked session, and exposes the command layer
//! to the React frontend as `#[tauri::command]` handlers. All real logic lives in
//! the verified `knows-me-core` library — these wrappers only assemble services
//! and translate `AppError` into a string for the IPC boundary.
//!
//! Build/run on a machine with the platform webview toolchain installed:
//! ```bash
//! npm install
//! npx tauri dev      # or: npx tauri build
//! ```

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod services;

use knows_me_core::core::commands::{self, AppStatus};
use knows_me_core::core::types::{
    AnswerInput, AnswerResult, AppConfig, DashboardDto, Draft, DraftRequest, GraphDto, GraphFilter,
    MiniHomeDto, PersonaReply, QueueItem, QueueItemId, QueueSort, SourceKind, TransferPolicy,
    TransferRecord,
};
use knows_me_core::AppState;
use services::Services;
use tauri::Manager;

type CmdResult<T> = Result<T, String>;

fn err(e: knows_me_core::AppError) -> String {
    e.to_string()
}

// --- U1: security & session ------------------------------------------------

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> AppStatus {
    commands::status(state.inner())
}

#[tauri::command]
async fn setup_password(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    password: String,
) -> CmdResult<()> {
    commands::setup_password(state.inner(), &password)
        .await
        .map_err(err)?;
    // First-run leaves the vault unlocked (KeyManager::setup), so the app routes
    // straight into the unlocked shell — assemble the services now, exactly as
    // unlock does, or every tab would greet a new user with "locked".
    services.activate(state.inner()).await;
    Ok(())
}

#[tauri::command]
async fn unlock(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    password: String,
) -> CmdResult<()> {
    commands::unlock(state.inner(), &password)
        .await
        .map_err(err)?;
    // Assemble the U3/U4 services now that the store is readable.
    services.activate(state.inner()).await;
    Ok(())
}

#[tauri::command]
async fn lock(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
) -> CmdResult<()> {
    services.deactivate().await;
    commands::lock(state.inner());
    Ok(())
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
async fn set_server_enabled(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    on: bool,
) -> CmdResult<()> {
    commands::set_server_enabled(state.inner(), on)
        .await
        .map_err(err)?;
    services.set_server_enabled(on).await;
    Ok(())
}

#[tauri::command]
fn list_transfers(state: tauri::State<'_, AppState>) -> Vec<TransferRecord> {
    commands::list_transfers(state.inner())
}

#[tauri::command]
async fn local_api_status(services: tauri::State<'_, Services>) -> CmdResult<Option<u16>> {
    Ok(services.local_api_port().await)
}

// --- U4: read views + persona (locked → AppError::Locked) ------------------

#[tauri::command]
async fn get_dashboard(services: tauri::State<'_, Services>) -> CmdResult<DashboardDto> {
    services
        .with(|s| async move { s.query.dashboard().await })
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_minihome(
    services: tauri::State<'_, Services>,
    limit: Option<usize>,
) -> CmdResult<MiniHomeDto> {
    services
        .with(|s| async move { s.query.minihome(limit).await })
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_graph(
    services: tauri::State<'_, Services>,
    filter: GraphFilter,
) -> CmdResult<GraphDto> {
    services
        .with(|s| async move { s.query.graph(filter).await })
        .await
        .map_err(err)
}

#[tauri::command]
async fn persona_chat(
    services: tauri::State<'_, Services>,
    prompt: String,
) -> CmdResult<PersonaReply> {
    services
        .with(|s| async move { s.persona.chat(prompt).await })
        .await
        .map_err(err)
}

#[tauri::command]
async fn persona_draft(
    services: tauri::State<'_, Services>,
    req: DraftRequest,
) -> CmdResult<Draft> {
    services
        .with(|s| async move { s.persona.draft(req).await })
        .await
        .map_err(err)
}

// --- U3: interview queue ----------------------------------------------------

#[tauri::command]
async fn queue_list(
    services: tauri::State<'_, Services>,
    sort: QueueSort,
) -> CmdResult<Vec<QueueItem>> {
    services
        .with(|s| async move { s.interview.list(sort).await })
        .await
        .map_err(err)
}

#[tauri::command]
async fn queue_answer(
    services: tauri::State<'_, Services>,
    id: QueueItemId,
    input: AnswerInput,
) -> CmdResult<AnswerResult> {
    services
        .with(|s| async move { s.interview.answer(id, input).await })
        .await
        .map_err(err)
}

// --- U2: collection -----------------------------------------------------------

/// What one triggered sync produced. Combines the collection counts
/// (`IngestReport`) with what processing made of them, so the UI can say
/// "collected 12, 3 facts, 2 questions" rather than just "done".
#[derive(serde::Serialize)]
struct IngestSummary {
    collected: usize,
    skipped: usize,
    errors: usize,
    facts_created: usize,
    queue_items_created: usize,
    filtered: usize,
}

#[tauri::command]
async fn trigger_ingest(
    services: tauri::State<'_, Services>,
    source: Option<SourceKind>,
) -> CmdResult<IngestSummary> {
    services
        .with(|s| async move {
            let ingest = s.ingestion.trigger(source).await?;
            // Ingestion feeds the sink synchronously, so by the time `trigger`
            // returns, processing for those items is done and the totals are
            // ready to collect.
            let processed = s.sink.take();
            Ok(IngestSummary {
                collected: ingest.collected,
                skipped: ingest.skipped,
                errors: ingest.errors,
                facts_created: processed.facts_created,
                queue_items_created: processed.queue_items_created,
                filtered: processed.filtered,
            })
        })
        .await
        .map_err(err)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Per-user encrypted data lives under the OS app-data directory.
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            app.manage(AppState::new(data_dir));
            app.manage(Services::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            setup_password,
            unlock,
            lock,
            get_config,
            set_transfer_policy,
            set_server_enabled,
            list_transfers,
            local_api_status,
            get_dashboard,
            get_minihome,
            get_graph,
            persona_chat,
            persona_draft,
            queue_list,
            queue_answer,
            trigger_ingest,
        ])
        .run(tauri::generate_context!())
        .expect("error while running knows-me");
}
