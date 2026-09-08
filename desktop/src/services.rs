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

use knows_me_core::core::traits::{EncryptedStore, InterviewApi, KnowledgeApi, Masker, PersonaApi};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
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
            store,
            knowledge.clone(),
            masker.clone(),
            llm.clone(),
        ));

        let query = Arc::new(QueryService::new(knowledge.clone()));
        let persona: Arc<dyn PersonaApi> =
            Arc::new(PersonaService::new(knowledge.clone(), masker, llm));

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
    pub async fn activate(&self, state: &AppState) {
        let set = ServiceSet::build(state).await;
        let mut guard = self.0.lock().await;
        if let Some(mut old) = guard.take() {
            if let Some(handle) = old.local_api.as_mut() {
                handle.stop().await;
            }
        }
        *guard = Some(set);
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
        let guard = self.0.lock().await;
        let set = guard
            .as_ref()
            .ok_or(knows_me_core::core::error::AppError::Locked)?;
        f(ServiceRef {
            knowledge: set.knowledge.clone(),
            interview: set.interview.clone(),
            query: set.query.clone(),
            persona: set.persona.clone(),
        })
        .await
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
}
