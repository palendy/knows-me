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

use knows_me_core::core::commands::{self, AppStatus, SourceStatus};
use knows_me_core::core::types::{
    AnswerInput, AnswerResult, AppConfig, ChatTurn, DashboardDto, Draft, DraftRequest, GraphDto,
    GraphFilter,
    MiniHomeDto, PersonaReply, QueueItem, QueueItemId, QueueSort, SourceKind, TransferPolicy,
    TransferRecord,
};
use knows_me_core::AppState;
use services::Services;
use tauri::{Emitter, Manager};

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
    let mut config = state.config();
    // The stored `llm_model` is a provider-agnostic default; surface the model
    // actually in effect (provider + key + env) so the settings screen doesn't
    // claim "claude-opus-5" while a cloud call really hits Gemini/OpenRouter.
    config.llm_model = knows_me_core::llm::active_model_label();
    config
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
    // Prior turns, oldest first. The frontend owns the thread; the backend
    // stays stateless so a locked/unlocked cycle cannot strand a conversation.
    history: Option<Vec<ChatTurn>>,
) -> CmdResult<PersonaReply> {
    let history = history.unwrap_or_default();
    services
        .with(|s| async move { s.persona.chat(prompt, history).await })
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

// --- U2: source connection -----------------------------------------------------

/// The connection status of every catalog source (credential fields + whether
/// each is connected/ready). Requires an unlocked vault since it reads the
/// encrypted credential store. Secret values are never returned.
#[tauri::command]
async fn list_sources(state: tauri::State<'_, AppState>) -> CmdResult<Vec<SourceStatus>> {
    commands::list_sources(state.inner()).await.map_err(err)
}

/// Store the credential for a source (connect). `values` is the source-specific
/// field map declared by its `FieldSpec`s.
#[tauri::command]
async fn connect_source(
    state: tauri::State<'_, AppState>,
    source: SourceKind,
    values: serde_json::Value,
) -> CmdResult<String> {
    commands::connect_source(state.inner(), source, values)
        .await
        .map_err(err)
}

/// Remove a source's credential (disconnect). Idempotent.
#[tauri::command]
async fn disconnect_source(
    state: tauri::State<'_, AppState>,
    source: SourceKind,
) -> CmdResult<()> {
    commands::disconnect_source(state.inner(), source)
        .await
        .map_err(err)
}

// --- U4: persona conversation persistence -------------------------------------

/// One persisted turn. Lives in the desktop crate because it is UI state — what
/// the owner saw on screen, sources included — not a domain object.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredTurn {
    role: knows_me_core::core::types::ChatRole,
    text: String,
    #[serde(default)]
    sources: Vec<knows_me_core::core::types::FactRef>,
    #[serde(default)]
    error: Option<String>,
}

const CHAT_NS: &str = "persona";
const CHAT_KEY: &str = "history";

/// The saved thread, or empty if there is none yet.
///
/// Goes through the encrypted store rather than browser storage: a conversation
/// with the persona is the owner's context in the clear, and "everything at rest
/// is encrypted" is the invariant the whole app rests on. Reading requires the
/// vault to be unlocked, which is the right gate.
#[tauri::command]
async fn persona_history_load(state: tauri::State<'_, AppState>) -> CmdResult<Vec<StoredTurn>> {
    use knows_me_core::core::traits::EncryptedStore;
    let bytes = state.store().get(CHAT_NS, CHAT_KEY).await.map_err(err)?;
    match bytes {
        None => Ok(vec![]),
        Some(b) => serde_json::from_slice(&b).map_err(|e| format!("chat history: {e}")),
    }
}

#[tauri::command]
async fn persona_history_save(
    state: tauri::State<'_, AppState>,
    turns: Vec<StoredTurn>,
) -> CmdResult<()> {
    use knows_me_core::core::traits::EncryptedStore;
    let bytes = serde_json::to_vec(&turns).map_err(|e| format!("chat history: {e}"))?;
    state
        .store()
        .put(CHAT_NS, CHAT_KEY, &bytes)
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
    /// Items left for a later run, so the owner can tell progress from repetition.
    remaining: usize,
    /// Why each error happened (e.g. "Notion: notion search 401: …"), so the UI
    /// can distinguish a bad token from "nothing new to collect".
    error_messages: Vec<String>,
    facts_created: usize,
    queue_items_created: usize,
    filtered: usize,
}

/// One progress tick emitted to the frontend during a sync.
#[derive(Clone, serde::Serialize)]
struct IngestProgress {
    source: SourceKind,
    done: usize,
    /// Best-known total; 0 means "unknown yet" (connector still discovering).
    total: usize,
}

/// A [`ProgressReporter`] that forwards each tick to the webview as an
/// `ingest://progress` event so the UI can drive a real progress bar.
struct EmitProgress {
    window: tauri::WebviewWindow,
}

impl knows_me_core::core::traits::ProgressReporter for EmitProgress {
    fn progress(&self, source: SourceKind, done: usize, total: usize) {
        // Emit failures (webview gone) are non-fatal — the sync still completes.
        let _ = self.window.emit(
            "ingest://progress",
            IngestProgress {
                source,
                done,
                total,
            },
        );
    }
}

#[tauri::command]
async fn trigger_ingest(
    services: tauri::State<'_, Services>,
    window: tauri::WebviewWindow,
    source: Option<SourceKind>,
) -> CmdResult<IngestSummary> {
    let reporter = EmitProgress { window };
    services
        .with(|s| async move {
            let ingest = s.ingestion.trigger(source, &reporter).await?;
            // Ingestion feeds the sink synchronously, so by the time `trigger`
            // returns, processing for those items is done and the totals are
            // ready to collect.
            let processed = s.sink.take();
            Ok(IngestSummary {
                collected: ingest.collected,
                skipped: ingest.skipped,
                errors: ingest.errors,
                remaining: ingest.remaining,
                error_messages: ingest.error_messages,
                facts_created: processed.facts_created,
                queue_items_created: processed.queue_items_created,
                filtered: processed.filtered,
            })
        })
        .await
        .map_err(err)
}

fn main() {
    // Load a local `.env` (if present) before anything reads the environment,
    // so the OpenRouter/Gemini key + model are picked up at startup without the
    // user exporting env vars. Missing file is fine — the LLM gateway falls back
    // to the offline canned client. Only compiled into `llm-http` builds.
    #[cfg(feature = "llm-http")]
    {
        let _ = dotenvy::dotenv();
    }

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
            list_sources,
            connect_source,
            disconnect_source,
            trigger_ingest,
            persona_history_load,
            persona_history_save,
        ])
        .run(tauri::generate_context!())
        .expect("error while running knows-me");
}
