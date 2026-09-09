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

use knows_me_core::core::traits::{
    CredentialStore, EncryptedStore, IngestionApi, InterviewApi, KeyManager, KnowledgeApi, Masker,
    PersonaApi,
};
use knows_me_core::ingestion::connectors::{FileConnector, GmailConnector, NotionConnector, SessionConnector};
use knows_me_core::ingestion::{ConnectorRegistry, IngestionCursorStore, IngestionService};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::processing::service::AlwaysOnline;
use knows_me_core::processing::{PendingQueue, ProcessingService, ProcessingSink, TransferLog};
use knows_me_core::persona::{
    LocalApiHandle, LocalApiServer, PersonaService, QueryService, DEFAULT_PORT,
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
        let knowledge: Arc<dyn KnowledgeApi> = knowledge_svc.clone();

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
        registry.register(Arc::new(SessionConnector::from_config(None)));
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

        ServiceSet {
            knowledge,
            interview,
            query,
            persona,
            ingestion,
            sink,
            local_api,
        }
    }
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
        } else if let Some(handle) = set.local_api.as_mut() {
            handle.stop().await;
        }
    }

    /// Tear down services on lock, gracefully stopping the local API.
    pub async fn deactivate(&self) {
        let mut guard = self.0.lock().await;
        if let Some(mut set) = guard.take() {
            if let Some(handle) = set.local_api.as_mut() {
                handle.stop().await;
            }
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
    #[allow(dead_code)]
    pub knowledge: Arc<dyn KnowledgeApi>,
    pub interview: Arc<dyn InterviewApi>,
    pub query: Arc<QueryService>,
    pub persona: Arc<dyn PersonaApi>,
    pub ingestion: Arc<dyn IngestionApi>,
    pub sink: Arc<ProcessingSink>,
}
