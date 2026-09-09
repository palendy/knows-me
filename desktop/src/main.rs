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
    AnswerInput, AnswerResult, AppConfig, Category, ChatTurn, DashboardDto, Draft, DraftRequest,
    Fact, FactId, GraphDto, GraphFilter, MiniHomeDto, PersonaReply, QueueItem, QueueItemId,
    QueueSort, SourceConfig, SourceKind, TransferPolicy, TransferRecord, Visibility,
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
    let provider = state.config().llm_provider;
    apply_api_key_to_env(state.inner(), &provider).await?;
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
    // `commands::unlock` restored the config and mirrored its non-secret LLM
    // selection to the environment; now push the stored API key too (the store
    // is readable) before building the client that reads it.
    let provider = state.config().llm_provider;
    apply_api_key_to_env(state.inner(), &provider).await?;
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

/// The config the settings screen reads. It carries the raw persisted fields
/// (so the edit form shows what was actually saved) plus two derived, read-only
/// signals: `llm_label` — the model *actually* in effect right now (provider +
/// key + env), and `has_api_key` — whether a key is stored for the current HTTP
/// provider, so the form can show "saved" without ever echoing the secret back.
#[derive(serde::Serialize)]
struct ConfigDto {
    #[serde(flatten)]
    config: AppConfig,
    /// Human-readable description of the live backend, e.g.
    /// "claude-sonnet-5 (로컬 Claude Code)".
    llm_label: String,
    /// Whether an API key is stored for the selected HTTP provider. Never the
    /// key itself — secrets are write-only across the IPC boundary.
    has_api_key: bool,
}

/// Encrypted-store location for the LLM API keys. Kept out of `AppConfig` (which
/// is non-secret and JSON-serialized to the frontend) so a key never crosses the
/// IPC boundary or lands in a config snapshot.
const LLM_NS: &str = "llm";

/// The store key holding the API secret for one HTTP provider.
fn api_key_slot(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" => Some("openai_api_key"),
        "anthropic" => Some("anthropic_api_key"),
        // The CLI backend carries its own auth — no key to store.
        _ => None,
    }
}

/// Read the stored API key for `provider`, if any. Returns `None` for the CLI
/// backend (no key slot) and when nothing is stored yet.
async fn load_api_key(state: &AppState, provider: &str) -> Result<Option<String>, String> {
    use knows_me_core::core::traits::EncryptedStore;
    let Some(slot) = api_key_slot(provider) else {
        return Ok(None);
    };
    let bytes = state.store().get(LLM_NS, slot).await.map_err(err)?;
    Ok(bytes.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

/// Push the stored API key for `provider` into the process environment (or clear
/// it when none is stored) so the gateway's `from_env` client build sees it.
/// Non-secret selection is applied separately by `AppConfig::apply_to_env`.
async fn apply_api_key_to_env(state: &AppState, provider: &str) -> Result<(), String> {
    let (anthropic, openai) = match provider.trim().to_ascii_lowercase().as_str() {
        "openai" => (None, load_api_key(state, "openai").await?),
        "anthropic" => (load_api_key(state, "anthropic").await?, None),
        _ => (None, None),
    };
    set_or_clear_env("ANTHROPIC_API_KEY", anthropic.as_deref());
    set_or_clear_env("OPENAI_API_KEY", openai.as_deref());
    Ok(())
}

fn set_or_clear_env(var: &str, value: Option<&str>) {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => std::env::set_var(var, v),
        None => std::env::remove_var(var),
    }
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> CmdResult<ConfigDto> {
    let config = state.config();
    let has_api_key = load_api_key(state.inner(), &config.llm_provider)
        .await?
        .is_some_and(|k| !k.trim().is_empty());
    Ok(ConfigDto {
        llm_label: knows_me_core::llm::active_model_label(),
        has_api_key,
        config,
    })
}

/// Persist the LLM selection and (optionally) its API key, then rebuild the
/// live services so the change takes effect immediately.
///
/// `api_key` is write-only: `Some` replaces the stored secret, `None` leaves it
/// untouched (so re-saving the form without retyping the key keeps it). The key
/// goes to the encrypted store, never into `AppConfig`.
#[tauri::command]
async fn set_llm_config(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    provider: String,
    model: String,
    base_url: Option<String>,
    api_key: Option<String>,
    // For the `claude-cli` backend: which local install to drive, as a command
    // line ("claude", a path, or "wsl -d <distro> claude"). Ignored otherwise.
    binary: Option<String>,
) -> CmdResult<()> {
    use knows_me_core::core::traits::EncryptedStore;

    // 1. Store the key first (if one was supplied), so the env reflects it.
    if let (Some(slot), Some(key)) = (api_key_slot(&provider), api_key.as_deref()) {
        // A blank submission clears the stored key rather than saving "".
        if key.trim().is_empty() {
            state.store().delete(LLM_NS, slot).await.map_err(err)?;
        } else {
            state
                .store()
                .put(LLM_NS, slot, key.trim().as_bytes())
                .await
                .map_err(err)?;
        }
    }

    // 2. Persist the non-secret selection (also mirrors it to the environment).
    let mut cfg = state.config();
    cfg.llm_provider = provider.clone();
    cfg.llm_model = model;
    cfg.llm_base_url = base_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    cfg.llm_binary = binary
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    state.save_config(cfg).await.map_err(err)?;

    // 3. Reflect the (possibly just-changed) key for the selected provider.
    apply_api_key_to_env(state.inner(), &provider).await?;

    // 4. Rebuild the services so the next cloud call uses the new client.
    services.activate(state.inner()).await;
    Ok(())
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

/// Detect the local Claude Code CLI installs (native + WSL distros) for the
/// settings picker. Runs `where`/`which`/`wsl` under the hood, so it's done on a
/// blocking thread to keep the async runtime free.
#[tauri::command]
async fn discover_claude_installs() -> CmdResult<Vec<knows_me_core::llm::ClaudeInstall>> {
    Ok(
        tokio::task::spawn_blocking(knows_me_core::llm::discover_claude_installs)
            .await
            .unwrap_or_default(),
    )
}

// --- Sharing (MCP): servers, tunnel, consumer tokens -----------------------

/// Runtime sharing status the settings screen reads. Config-level (`enabled`)
/// plus the live server/tunnel state, and whether cloudflared is installed so
/// the UI can tell "tunnel off" from "can't tunnel".
#[derive(serde::Serialize)]
struct ShareStatusDto {
    enabled: bool,
    owner_port: Option<u16>,
    shared_port: Option<u16>,
    tunnel_url: Option<String>,
    cloudflared_installed: bool,
}

/// A freshly issued consumer token. `secret` is shown to the owner **once** —
/// it is never recoverable, so the UI must surface it immediately.
#[derive(serde::Serialize)]
struct IssuedTokenDto {
    id: String,
    secret: String,
}

/// A live token's public metadata for the tokens list. Never the secret.
#[derive(serde::Serialize)]
struct ShareTokenDto {
    id: String,
    granted: Vec<String>,
    issued_at: String,
}

#[tauri::command]
async fn set_sharing_enabled(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    on: bool,
) -> CmdResult<()> {
    commands::set_sharing_enabled(state.inner(), on)
        .await
        .map_err(err)?;
    services.set_sharing_enabled(on).await;
    Ok(())
}

/// cloudflared install status, probed once per session and cached. The probe
/// spawns a `cloudflared --version` child, and the sharing UI polls `share_status`
/// on mount and after every mutation — install status doesn't change within a
/// session, so forking a process per poll is wasteful. (Trade-off: installing
/// cloudflared while the app runs isn't reflected until the next launch.)
static CLOUDFLARED_INSTALLED: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

#[tauri::command]
async fn share_status(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
) -> CmdResult<ShareStatusDto> {
    let (owner_port, shared_port) = services.mcp_ports().await;
    let cloudflared_installed = *CLOUDFLARED_INSTALLED
        .get_or_init(knows_me_core::sharing::tunnel::cloudflared_available)
        .await;
    Ok(ShareStatusDto {
        enabled: state.config().sharing_enabled,
        owner_port,
        shared_port,
        tunnel_url: services.tunnel_url().await,
        cloudflared_installed,
    })
}

/// The owner's category vocabulary — the grant choices the issuance form offers.
#[tauri::command]
async fn list_share_categories(services: tauri::State<'_, Services>) -> CmdResult<Vec<String>> {
    services.owner_categories().await.map_err(err)
}

/// Issue a consumer token for `id`, granting the given (normalized) categories.
/// Returns the one-time secret.
#[tauri::command]
async fn issue_share_token(
    services: tauri::State<'_, Services>,
    id: String,
    categories: Vec<String>,
) -> CmdResult<IssuedTokenDto> {
    // Normalize/validate every category before minting — a bad one is a rejected
    // request, not a token that grants nothing.
    let cats = categories
        .iter()
        .map(|c| Category::parse(c))
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let issued = services.issue_token(id, cats).await.map_err(err)?;
    Ok(IssuedTokenDto {
        id: issued.id,
        secret: issued.secret,
    })
}

/// Revoke every live token issued under `id`. Reports whether any changed.
#[tauri::command]
async fn revoke_share_token(services: tauri::State<'_, Services>, id: String) -> CmdResult<bool> {
    services.revoke_token(&id).await.map_err(err)
}

#[tauri::command]
async fn list_share_tokens(services: tauri::State<'_, Services>) -> CmdResult<Vec<ShareTokenDto>> {
    let tokens = services.list_tokens().await.map_err(err)?;
    Ok(tokens
        .into_iter()
        .map(|t| ShareTokenDto {
            id: t.id,
            granted: t.granted.iter().map(|c| c.as_str().to_string()).collect(),
            issued_at: t.issued_at.to_rfc3339(),
        })
        .collect())
}

/// Start a cloudflared quick tunnel over the shared listener; returns its URL.
#[tauri::command]
async fn start_share_tunnel(services: tauri::State<'_, Services>) -> CmdResult<String> {
    services.start_tunnel().await.map_err(err)
}

#[tauri::command]
async fn stop_share_tunnel(services: tauri::State<'_, Services>) -> CmdResult<()> {
    services.stop_tunnel().await;
    Ok(())
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

/// Fetch a single fact (owner scope) so the wiki inspector can show and edit its
/// sharing state (visibility + category).
#[tauri::command]
async fn get_fact(services: tauri::State<'_, Services>, id: FactId) -> CmdResult<Fact> {
    services
        .with(|s| async move { s.knowledge.get(id).await })
        .await
        .map_err(err)
}

/// Set a fact's sharing state: its `visibility` and (normalized) `category`.
///
/// This is the owner's "approve-share + assign-category" gate — the single place
/// a page becomes reachable by a consumer token, which requires `Shared` AND a
/// granted category. `category` is validated/normalized via `Category::parse`;
/// `None` clears it.
#[tauri::command]
async fn set_fact_sharing(
    services: tauri::State<'_, Services>,
    id: FactId,
    visibility: Visibility,
    category: Option<String>,
) -> CmdResult<()> {
    let category = category
        .map(|c| Category::parse(&c))
        .transpose()
        .map_err(err)?;
    services
        .with(|s| async move { s.knowledge.set_sharing(id, visibility, category).await.map(|_| ()) })
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
async fn disconnect_source(state: tauri::State<'_, AppState>, source: SourceKind) -> CmdResult<()> {
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

/// The project directories the owner could collect from, with session counts.
///
/// Read straight off disk rather than from the vault: this answers "what is
/// available", which is true whether or not the vault has ever been unlocked
/// and must not be narrowed by the scope currently in force — a picker that
/// hides the projects you excluded gives you no way to put them back.
#[tauri::command]
async fn list_session_projects() -> Vec<knows_me_core::ingestion::connectors::SessionProject> {
    // A synchronous command runs on the Tauri runtime (main) thread, so this
    // unbounded `read_dir` + per-file `stat` scan froze the UI while the scope
    // dialog opened. Push it to a blocking thread like `discover_claude_installs`.
    tokio::task::spawn_blocking(
        knows_me_core::ingestion::connectors::SessionConnector::available_projects,
    )
    .await
    .unwrap_or_default()
}

/// Narrow (or reset) which project directories collection reads from.
///
/// An empty list means "everything I can find" — the natural reading of a
/// picker with nothing ticked, and the only one that cannot strand the owner
/// with a source that silently collects nothing.
#[tauri::command]
async fn set_session_scope(
    state: tauri::State<'_, AppState>,
    services: tauri::State<'_, Services>,
    roots: Vec<String>,
) -> CmdResult<()> {
    use knows_me_core::core::traits::EncryptedStore;
    let config = serde_json::json!({ "roots": roots });
    let bytes = serde_json::to_vec(&config).map_err(|e| format!("scope: {e}"))?;
    state
        .store()
        .put(services::SCOPE_NS, services::SCOPE_KEY, &bytes)
        .await
        .map_err(err)?;

    // Persist first, then apply: a scope that survives the restart but was not
    // applied is a confusing next run; the reverse is a lost setting.
    services
        .with(|s| async move {
            s.ingestion
                .configure(SourceKind::Session, SourceConfig(config))
                .await
        })
        .await
        .map_err(err)
}

/// The scope currently in force, as a list of root paths. Empty = all.
#[tauri::command]
async fn get_session_scope(state: tauri::State<'_, AppState>) -> CmdResult<Vec<String>> {
    use knows_me_core::core::traits::EncryptedStore;
    let bytes = state
        .store()
        .get(services::SCOPE_NS, services::SCOPE_KEY)
        .await
        .map_err(err)?;
    let Some(bytes) = bytes else {
        return Ok(vec![]);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    Ok(value
        .get("roots")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Collect until the source has nothing left, instead of one capped batch.
///
/// One press of the ordinary collect button takes a bounded slice so it stays
/// predictable. That cap was also the ceiling on what the vault could ever
/// hold — with hundreds of transcripts on disk, everything past the newest
/// batch was reachable only by pressing the button over and over. This is the
/// button for "just do all of it"; it can run for a long time, which is why
/// progress is emitted per pass.
#[tauri::command]
async fn trigger_ingest_all(
    services: tauri::State<'_, Services>,
    window: tauri::WebviewWindow,
    source: Option<SourceKind>,
) -> CmdResult<IngestSummary> {
    const MAX_PASSES: usize = 200;
    let reporter = EmitProgress { window };
    services
        .with(|s| async move {
            let ingest = s
                .ingestion
                .trigger_all(source, &reporter, MAX_PASSES)
                .await?;
            let processed = s.sink.take();
            eprintln!(
                "[ingest-all] source={source:?} collected={} skipped={} → facts_created={} queue_items={} filtered={}",
                ingest.collected,
                ingest.skipped,
                processed.facts_created,
                processed.queue_items_created,
                processed.filtered
            );
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
            eprintln!(
                "[ingest] source={source:?} collected={} skipped={} → facts_created={} queue_items={} filtered={}",
                ingest.collected,
                ingest.skipped,
                processed.facts_created,
                processed.queue_items_created,
                processed.filtered
            );
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
    // so LLM settings are picked up at startup without the user exporting env
    // vars. Missing file is fine — every setting here has a default.
    //
    // Unconditional, not gated on `llm-http`: the default backend is the local
    // Claude CLI, which reads `CLAUDE_CLI_MODEL`/`CLAUDE_CLI_BINARY` from the
    // environment and needs no HTTP feature at all. Behind the feature gate,
    // a `.env` in the default build was read by nothing and the owner's chosen
    // model was silently ignored.
    let _ = dotenvy::dotenv();

    // Startup diagnostic: which LLM the processing pipeline will actually use.
    // A summarizer that returns NOTHING for thin pages is the difference between
    // a Notion page becoming a dashboard fact or being filtered out.
    eprintln!(
        "[llm] active backend: {}",
        knows_me_core::llm::active_model_label()
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            set_llm_config,
            set_server_enabled,
            list_transfers,
            local_api_status,
            discover_claude_installs,
            set_sharing_enabled,
            share_status,
            list_share_categories,
            issue_share_token,
            revoke_share_token,
            list_share_tokens,
            start_share_tunnel,
            stop_share_tunnel,
            get_dashboard,
            get_minihome,
            get_graph,
            get_fact,
            set_fact_sharing,
            persona_chat,
            persona_draft,
            queue_list,
            queue_answer,
            list_sources,
            connect_source,
            disconnect_source,
            trigger_ingest,
            trigger_ingest_all,
            list_session_projects,
            get_session_scope,
            set_session_scope,
            persona_history_load,
            persona_history_save,
        ])
        .run(tauri::generate_context!())
        .expect("error while running knows-me");
}
