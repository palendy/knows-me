//! Runtime service registry for the unlocked session.
//!
//! U1's [`AppState`] owns the security primitives (key manager, encrypted store,
//! masker, transfer log) and stays unchanged. The U3/U4 services, by contrast,
//! are only valid *while unlocked* — they read through the encrypted store, which
//! needs the derived key. So we keep them here in a separate managed state that
//! is populated on `unlock` and cleared on `lock`.
//!
//! Assembly order mirrors the dependency graph:
//! `KnowledgeService(store)` → shared `Arc` → `InterviewService`, `QueryService`,
//! `PersonaService`. The LLM client comes from [`knows_me_core::llm::build_client`]
//! (canned offline unless the `llm-http` feature + provider key are present).

use std::sync::Arc;

use knows_me_core::core::error::{AppError, Result};
use knows_me_core::core::traits::{
    Connector, CredentialStore, EncryptedStore, IngestionApi, InterviewApi, KeyManager,
    KnowledgeApi, Masker, PersonaApi,
};
use knows_me_core::core::types::{Category, SourceConfig};
use knows_me_core::ingestion::connectors::{
    FileConnector, GmailConnector, NotionConnector, SessionConnector,
};
use knows_me_core::ingestion::{ConnectorRegistry, IngestionCursorStore, IngestionService};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::persona::{
    LocalApiHandle, LocalApiServer, PersonaService, QueryService, DEFAULT_PORT,
};
use knows_me_core::processing::service::AlwaysOnline;
use knows_me_core::processing::{PendingQueue, ProcessingService, ProcessingSink, TransferLog};
use knows_me_core::sharing::mcp::{McpHandle, McpServer, DEFAULT_PORT as MCP_DEFAULT_PORT};
use knows_me_core::sharing::{
    AccessError, IssuedToken, KnowledgeSharing, QuickTunnel, SharingApi, Token, TokenInfo,
    TokenStore, TunnelHandle,
};
use knows_me_core::AppState;
use tokio::sync::Mutex;

/// The set of services available during an unlocked session.
pub struct ServiceSet {
    pub knowledge: Arc<dyn KnowledgeApi>,
    pub interview: Arc<dyn InterviewApi>,
    pub query: Arc<QueryService>,
    pub persona: Arc<dyn PersonaApi>,
    /// U2 collection orchestrator (Session/File/Notion/Gmail connectors).
    pub ingestion: Arc<dyn IngestionApi>,
    /// The sink ingestion feeds. Held so a triggered sync can report how many
    /// facts/questions the processing pass actually produced.
    pub sink: Arc<ProcessingSink>,
    /// Local REST server handle. `Some` while running; dropping it stops the
    /// server, so it must live here for the life of the session.
    pub local_api: Option<LocalApiHandle>,
    /// Sharing surface over the same knowledge store — backs both MCP listeners
    /// and the token-issuance UI. Always built; only the servers below are gated.
    pub sharing: Arc<KnowledgeSharing>,
    /// Consumer-token vault (over the encrypted store). Always built so tokens can
    /// be issued/revoked even before the servers are turned on.
    pub tokens: Arc<TokenStore>,
    /// Owner-loopback MCP listener (step ⓐ). `Some` while sharing is enabled.
    pub mcp_owner: Option<McpHandle>,
    /// Bearer-only shared MCP listener (step ⓑ), the surface a tunnel fronts.
    pub mcp_shared: Option<McpHandle>,
    /// cloudflared quick tunnel fronting the shared listener. `Some` while up.
    /// Dropping it kills cloudflared, so it must live here for the session.
    pub tunnel: Option<TunnelHandle>,
}

impl ServiceSet {
    /// Assemble the services over the (now-unlocked) core components. If the
    /// server is enabled in config, also starts the loopback API.
    async fn build(state: &AppState) -> ServiceSet {
        let store: Arc<dyn EncryptedStore> = state.store();
        let masker: Arc<dyn Masker> = state.masker();
        let llm = knows_me_core::llm::build_client(state.transfer_log());

        let knowledge_svc = Arc::new(KnowledgeService::new(store.clone()));
        // Rebuild the search index from the decrypted store now that we can read.
        knowledge_svc.build_index().await.ok();

        // Local-dev housekeeping (opt-in via env, debug builds only): drop the
        // previously-collected fake Gmail facts and forget their seen-markers so
        // the next "collect" re-runs the fixtures cleanly through the normal
        // pipeline instead of being skipped as duplicates. No-op without the env.
        #[cfg(debug_assertions)]
        if std::env::var("KNOWSME_RESET_GMAIL").as_deref() == Ok("1") {
            use knows_me_core::core::types::SourceKind;
            match knowledge_svc.delete_facts_from_source(SourceKind::Gmail).await {
                Ok(n) => eprintln!("[dev-reset] removed {n} Gmail fact(s)"),
                Err(e) => eprintln!("[dev-reset] fact purge failed: {e}"),
            }
            let cursors = IngestionCursorStore::new(store.clone());
            match cursors.clear_seen(SourceKind::Gmail).await {
                Ok(n) => eprintln!("[dev-reset] cleared {n} Gmail seen-marker(s)"),
                Err(e) => eprintln!("[dev-reset] seen clear failed: {e}"),
            }
        }

        // Local-dev housekeeping for Notion: `KNOWSME_NOTION_RESET_N=<n>` drops
        // the previously-collected Notion facts, forgets their seen-markers, and
        // clears the incremental cursor so the next "collect" re-pulls pages from
        // scratch. The connector caps that run at N pages (see
        // `notion::http::max_pages_per_sync`), so exactly the oldest N re-flow
        // through the real processing pipeline — enough to verify the
        // collect → process → dashboard path a few items at a time. No-op without
        // the env; release builds never take this branch.
        #[cfg(debug_assertions)]
        if std::env::var("KNOWSME_NOTION_RESET_N")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .is_some_and(|n| n > 0)
        {
            use knows_me_core::core::types::SourceKind;
            match knowledge_svc.delete_facts_from_source(SourceKind::Notion).await {
                Ok(n) => eprintln!("[dev-reset] removed {n} Notion fact(s)"),
                Err(e) => eprintln!("[dev-reset] fact purge failed: {e}"),
            }
            let cursors = IngestionCursorStore::new(store.clone());
            match cursors.clear_seen(SourceKind::Notion).await {
                Ok(n) => eprintln!("[dev-reset] cleared {n} Notion seen-marker(s)"),
                Err(e) => eprintln!("[dev-reset] seen clear failed: {e}"),
            }
            match cursors.clear_cursor(SourceKind::Notion).await {
                Ok(()) => eprintln!("[dev-reset] cleared Notion cursor"),
                Err(e) => eprintln!("[dev-reset] cursor clear failed: {e}"),
            }
        }

        let knowledge: Arc<dyn KnowledgeApi> = knowledge_svc.clone();

        // Sharing surface (MCP tools + token issuance) over the same concrete
        // knowledge service, plus the consumer-token vault over the encrypted
        // store. Both are built regardless of the toggle; only the listeners
        // started below are gated on `sharing_enabled`.
        let sharing = Arc::new(KnowledgeSharing::new(knowledge_svc.clone()));
        let tokens = Arc::new(TokenStore::new(store.clone()));

        let interview: Arc<dyn InterviewApi> = Arc::new(InterviewService::new(
            store.clone(),
            knowledge.clone(),
            masker.clone(),
            llm.clone(),
        ));

        let query = Arc::new(QueryService::new(knowledge.clone()));
        let persona: Arc<dyn PersonaApi> = Arc::new(PersonaService::new(
            knowledge.clone(),
            masker.clone(),
            llm.clone(),
        ));

        // --- U2: collection + processing -----------------------------------
        // Processing is what turns raw items into facts/questions, so it is the
        // sink ingestion writes into. Both read through the same encrypted
        // store, so both are only valid while unlocked — same lifetime as the
        // rest of this set.
        let processing = Arc::new(ProcessingService::new(
            masker,
            llm,
            // U2 keeps its own store-backed transparency log, distinct from
            // U1's in-memory one behind `list_transfers`.
            Arc::new(TransferLog::new(store.clone())),
            knowledge.clone(),
            interview.clone(),
            Arc::new(PendingQueue::new(store.clone())),
            Arc::new(AlwaysOnline),
        ));
        let sink = Arc::new(ProcessingSink::new(processing));

        let credentials: Arc<dyn CredentialStore> = state.store();
        let mut registry = ConnectorRegistry::new();
        // Session transcripts are already on disk — no credentials needed, so
        // this one works the moment the vault opens.
        // Kept concretely as well as in the registry: the scope the owner picks
        // is applied to this connector and has to survive a restart, so the
        // shell replays the stored scope on unlock (see `apply_stored_scope`).
        let session = Arc::new(SessionConnector::from_config(None));
        registry.register(session.clone());
        // Replay the owner's saved collection scope before the first sync, so
        // an app that reopens collects what it was last told to collect.
        if let Some(cfg) = load_session_scope(&store).await {
            if let Err(e) = session.configure(&cfg) {
                eprintln!("[sources] stored session scope ignored: {e}");
            }
        }
        registry.register(Arc::new(FileConnector::new()));
        registry.register(Arc::new(NotionConnector::new(credentials.clone())));
        registry.register(Arc::new(GmailConnector::new(credentials)));

        let ingestion: Arc<dyn IngestionApi> = Arc::new(IngestionService::new(
            Arc::new(registry),
            Arc::new(IngestionCursorStore::new(store)),
            sink.clone(),
        ));

        let local_api = if state.config().server_enabled {
            start_local_api(persona.clone()).await
        } else {
            None
        };

        // The tunnel is never auto-started — it publishes a public URL, so it is an
        // explicit owner action (a command), not a side effect of unlocking.
        let (mcp_owner, mcp_shared) = if state.config().sharing_enabled {
            start_mcp_servers(sharing.clone(), tokens.clone()).await
        } else {
            (None, None)
        };

        ServiceSet {
            knowledge,
            interview,
            query,
            persona,
            ingestion,
            sink,
            local_api,
            sharing,
            tokens,
            mcp_owner,
            mcp_shared,
            tunnel: None,
        }
    }

    /// Gracefully stop every server this session started — the persona local API,
    /// both MCP listeners, and any tunnel. Used on lock and when discarding a set
    /// built for a vault that locked mid-build.
    async fn stop_servers(&mut self) {
        if let Some(h) = self.local_api.as_mut() {
            h.stop().await;
        }
        if let Some(h) = self.mcp_owner.as_mut() {
            h.stop().await;
        }
        if let Some(h) = self.mcp_shared.as_mut() {
            h.stop().await;
        }
        if let Some(t) = self.tunnel.as_mut() {
            t.stop().await;
        }
    }
}

/// Namespace/key for the owner's saved collection scope.
pub const SCOPE_NS: &str = "sources";
pub const SCOPE_KEY: &str = "session-scope";

/// The saved scope, or `None` when the owner has never narrowed it.
///
/// A missing or unreadable value is not an error: it means "collect from the
/// default locations", which is what a fresh install does anyway.
async fn load_session_scope(store: &Arc<dyn EncryptedStore>) -> Option<SourceConfig> {
    let bytes = store.get(SCOPE_NS, SCOPE_KEY).await.ok().flatten()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .map(SourceConfig)
}

/// Start the loopback persona API, logging (not failing) on bind errors — the
/// vault opening must not depend on a free port.
async fn start_local_api(persona: Arc<dyn PersonaApi>) -> Option<LocalApiHandle> {
    match LocalApiServer::start(persona, DEFAULT_PORT).await {
        Ok(handle) => {
            eprintln!("[local-api] listening on 127.0.0.1:{}", handle.port());
            Some(handle)
        }
        Err(e) => {
            eprintln!("[local-api] failed to start: {e}");
            None
        }
    }
}

/// Start both MCP listeners: the owner-loopback server (step ⓐ) and the
/// Bearer-only shared server (step ⓑ, the surface a tunnel fronts). Bind failures
/// are logged, not fatal — unlocking must not hinge on a free port. The shared
/// listener binds above the owner's *actual* bound port so the two never collide.
/// Start the owner-loopback MCP listener (step ⓐ). Bind failure is logged, not
/// fatal — unlocking must not hinge on a free port.
async fn start_owner_listener(
    sharing: Arc<KnowledgeSharing>,
    tokens: Arc<TokenStore>,
) -> Option<McpHandle> {
    match McpServer::start_owner(sharing, tokens, MCP_DEFAULT_PORT).await {
        Ok(h) => {
            eprintln!("[mcp] owner listening on 127.0.0.1:{}", h.port());
            Some(h)
        }
        Err(e) => {
            eprintln!("[mcp] owner failed to start: {e}");
            None
        }
    }
}

/// Start the Bearer-only shared MCP listener (step ⓑ) at `port`. Bind failure is
/// logged, not fatal.
async fn start_shared_listener(
    sharing: Arc<KnowledgeSharing>,
    tokens: Arc<TokenStore>,
    port: u16,
) -> Option<McpHandle> {
    match McpServer::start_shared(sharing, tokens, port).await {
        Ok(h) => {
            eprintln!("[mcp] shared listening on 127.0.0.1:{}", h.port());
            Some(h)
        }
        Err(e) => {
            eprintln!("[mcp] shared failed to start: {e}");
            None
        }
    }
}

/// The port to bind the shared listener on: one above the owner's *actual* bound
/// port so the two never collide (or the default+1 if the owner isn't up).
fn shared_port_for(owner: Option<&McpHandle>) -> u16 {
    owner
        .map(|h| h.port().saturating_add(1))
        .unwrap_or(MCP_DEFAULT_PORT.saturating_add(1))
}

/// Start both MCP listeners (owner ⓐ + shared ⓑ). Used at unlock; toggling
/// sharing on later starts each listener individually so a partial start can
/// recover (see [`Services::set_sharing_enabled`]).
async fn start_mcp_servers(
    sharing: Arc<KnowledgeSharing>,
    tokens: Arc<TokenStore>,
) -> (Option<McpHandle>, Option<McpHandle>) {
    let owner = start_owner_listener(sharing.clone(), tokens.clone()).await;
    let shared = start_shared_listener(sharing, tokens, shared_port_for(owner.as_ref())).await;
    (owner, shared)
}

/// Map a sharing [`AccessError`] onto the core [`AppError`] for the command
/// layer. Owner-scope reads realistically only ever hit `Locked`/`Unavailable`;
/// the rest are mapped defensively.
fn access_err(e: AccessError) -> AppError {
    match e {
        AccessError::Locked => AppError::Locked,
        AccessError::Unavailable => AppError::Io("지식 저장소에 연결할 수 없습니다".to_string()),
        AccessError::NotFound => AppError::NotFound("category".to_string()),
        AccessError::Unauthorized => AppError::Io("unauthorized".to_string()),
        AccessError::InvalidInput => AppError::InvalidInput("category".to_string()),
    }
}

/// Managed Tauri state holding the current session's services (empty while
/// locked). Guarded by an async mutex so command handlers can rebuild/tear down
/// without blocking the runtime.
#[derive(Default)]
pub struct Services(Mutex<Option<ServiceSet>>);

impl Services {
    /// Assemble services after a successful unlock, replacing any prior set.
    ///
    /// Tears down any prior set BEFORE building the new one so a re-activate
    /// can't have two servers contend for the same port (the old one would keep
    /// 8765 and push the new one to 8766). After building — which reads the store
    /// and may take a moment — it re-checks that the vault is still unlocked
    /// before installing, so a `lock` that lands mid-build doesn't leave live
    /// services behind a locked status.
    pub async fn activate(&self, state: &AppState) {
        // 1. Drop any prior session first (frees the local API port).
        self.deactivate().await;

        // 2. Build the new set (starts the local API on the now-free port).
        let mut set = ServiceSet::build(state).await;

        // 3. Install only if still unlocked; otherwise discard what we just built
        //    (and stop the server we may have started).
        let mut guard = self.0.lock().await;
        if state.key_manager().is_unlocked() {
            *guard = Some(set);
        } else {
            // Locked mid-build: discard, stopping every server we just started.
            set.stop_servers().await;
        }
    }

    /// Tear down services on lock, gracefully stopping every running server.
    pub async fn deactivate(&self) {
        let mut guard = self.0.lock().await;
        if let Some(mut set) = guard.take() {
            set.stop_servers().await;
        }
    }

    /// Toggle the local API server on the live session (config is persisted by
    /// the caller). No-op if the session is locked.
    pub async fn set_server_enabled(&self, on: bool) {
        let mut guard = self.0.lock().await;
        let Some(set) = guard.as_mut() else { return };
        match (on, set.local_api.is_some()) {
            (true, false) => set.local_api = start_local_api(set.persona.clone()).await,
            (false, true) => {
                if let Some(handle) = set.local_api.as_mut() {
                    handle.stop().await;
                }
                set.local_api = None;
            }
            _ => {}
        }
    }

    /// The port the local API is bound to, if running.
    pub async fn local_api_port(&self) -> Option<u16> {
        self.0
            .lock()
            .await
            .as_ref()
            .and_then(|s| s.local_api.as_ref())
            .map(|h| h.port())
    }

    // --- Sharing (MCP) --------------------------------------------------------

    /// Enable/disable the sharing (MCP) servers on the live session (config is
    /// persisted by the caller). No-op if locked. Turning sharing off also tears
    /// down any tunnel that was fronting the shared listener.
    pub async fn set_sharing_enabled(&self, on: bool) {
        let mut guard = self.0.lock().await;
        let Some(set) = guard.as_mut() else { return };
        if on {
            // Start whichever listener isn't already up — per-listener rather than
            // all-or-nothing, so a partial start recovers: if the owner port was
            // momentarily taken at unlock (owner None, shared Some), toggling off
            // then on brings the owner listener back without a lock/unlock cycle.
            if set.mcp_owner.is_none() {
                set.mcp_owner = start_owner_listener(set.sharing.clone(), set.tokens.clone()).await;
            }
            if set.mcp_shared.is_none() {
                let port = shared_port_for(set.mcp_owner.as_ref());
                set.mcp_shared =
                    start_shared_listener(set.sharing.clone(), set.tokens.clone(), port).await;
            }
        } else {
            // Turning sharing off tears down both listeners and any tunnel fronting
            // the shared one.
            if let Some(h) = set.mcp_owner.as_mut() {
                h.stop().await;
            }
            set.mcp_owner = None;
            if let Some(h) = set.mcp_shared.as_mut() {
                h.stop().await;
            }
            set.mcp_shared = None;
            if let Some(t) = set.tunnel.as_mut() {
                t.stop().await;
            }
            set.tunnel = None;
        }
    }

    /// `(owner-loopback port, shared port)` — each `Some` while that listener runs.
    pub async fn mcp_ports(&self) -> (Option<u16>, Option<u16>) {
        let guard = self.0.lock().await;
        match guard.as_ref() {
            Some(s) => (
                s.mcp_owner.as_ref().map(|h| h.port()),
                s.mcp_shared.as_ref().map(|h| h.port()),
            ),
            None => (None, None),
        }
    }

    /// The public tunnel URL fronting the shared listener, if a tunnel is up.
    pub async fn tunnel_url(&self) -> Option<String> {
        self.0
            .lock()
            .await
            .as_ref()
            .and_then(|s| s.tunnel.as_ref())
            .map(|t| t.url().to_string())
    }

    /// Issue a consumer token for `id` granting `categories`. Requires unlock.
    pub async fn issue_token(&self, id: String, categories: Vec<Category>) -> Result<IssuedToken> {
        self.tokens_arc().await?.issue(id, categories).await
    }

    /// Revoke every live token issued under `id`; reports whether any changed.
    pub async fn revoke_token(&self, id: &str) -> Result<bool> {
        self.tokens_arc().await?.revoke(id).await
    }

    /// List live tokens' public metadata (never secrets).
    pub async fn list_tokens(&self) -> Result<Vec<TokenInfo>> {
        self.tokens_arc().await?.list().await
    }

    /// The owner's own category vocabulary — the grant choices the issuance UI
    /// offers. Uses the owner token, so it spans Private+Shared categories.
    pub async fn owner_categories(&self) -> Result<Vec<String>> {
        let sharing = {
            let guard = self.0.lock().await;
            guard.as_ref().ok_or(AppError::Locked)?.sharing.clone()
        };
        sharing
            .list_categories(&Token::owner())
            .await
            .map_err(access_err)
    }

    /// Start a cloudflared quick tunnel fronting the shared MCP listener, returning
    /// its public URL. Requires sharing on (the shared listener running). Replaces
    /// any prior tunnel.
    pub async fn start_tunnel(&self) -> Result<String> {
        // Read the shared port up front; don't hold the lock across the spawn,
        // which awaits cloudflared for a few seconds.
        let shared_port = {
            let guard = self.0.lock().await;
            let set = guard.as_ref().ok_or(AppError::Locked)?;
            match set.mcp_shared.as_ref() {
                Some(h) => h.port(),
                None => {
                    return Err(AppError::InvalidInput(
                        "공유 서버가 꺼져 있습니다. 지식 공유를 먼저 켜세요.".into(),
                    ))
                }
            }
        };

        let handle = QuickTunnel::start(shared_port).await?;
        let url = handle.url().to_string();

        let mut guard = self.0.lock().await;
        match guard.as_mut() {
            Some(set) => {
                if let Some(mut old) = set.tunnel.take() {
                    old.stop().await;
                }
                set.tunnel = Some(handle);
                Ok(url)
            }
            // Locked while spawning — don't leak the cloudflared child.
            None => {
                let mut h = handle;
                h.stop().await;
                Err(AppError::Locked)
            }
        }
    }

    /// Stop the tunnel, if one is running. Idempotent.
    pub async fn stop_tunnel(&self) {
        let mut guard = self.0.lock().await;
        if let Some(set) = guard.as_mut() {
            if let Some(t) = set.tunnel.as_mut() {
                t.stop().await;
            }
            set.tunnel = None;
        }
    }

    /// The consumer-token vault of the active session, or `Locked`.
    async fn tokens_arc(&self) -> Result<Arc<TokenStore>> {
        Ok(self
            .0
            .lock()
            .await
            .as_ref()
            .ok_or(AppError::Locked)?
            .tokens
            .clone())
    }

    /// Run `f` against the active service set, or return `AppError::Locked` if
    /// the session is locked. Handlers use this so a locked vault uniformly
    /// yields the "unlock required" error at the IPC boundary.
    pub async fn with<T, F, Fut>(&self, f: F) -> knows_me_core::core::error::Result<T>
    where
        F: FnOnce(ServiceRef) -> Fut,
        Fut: std::future::Future<Output = knows_me_core::core::error::Result<T>>,
    {
        // Hold the lock only long enough to clone the (cheap Arc) handles, then
        // drop the guard BEFORE awaiting `f`. Otherwise a slow command — e.g. a
        // 30s persona_chat hitting the cloud LLM — would keep the mutex and stall
        // every other command (including the offline read views) behind it.
        let refs = {
            let guard = self.0.lock().await;
            let set = guard
                .as_ref()
                .ok_or(knows_me_core::core::error::AppError::Locked)?;
            ServiceRef {
                knowledge: set.knowledge.clone(),
                interview: set.interview.clone(),
                query: set.query.clone(),
                persona: set.persona.clone(),
                ingestion: set.ingestion.clone(),
                sink: set.sink.clone(),
            }
        };
        f(refs).await
    }
}

/// Cheap, cloneable handles to the services for use inside a [`Services::with`]
/// closure. All owned `Arc`s so the closure's future can outlive the lock guard
/// without borrowing the [`ServiceSet`].
pub struct ServiceRef {
    pub knowledge: Arc<dyn KnowledgeApi>,
    pub interview: Arc<dyn InterviewApi>,
    pub query: Arc<QueryService>,
    pub persona: Arc<dyn PersonaApi>,
    pub ingestion: Arc<dyn IngestionApi>,
    pub sink: Arc<ProcessingSink>,
}
