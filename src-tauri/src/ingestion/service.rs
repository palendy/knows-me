//! [`IngestionService`] — implements U1's [`IngestionApi`].
//!
//! Orchestrates connectors with the idempotency gate, per-source error isolation
//! (Pattern P5), and a single-flight run lock (Pattern P6). Emits new `RawItem`s
//! to a sink (the ProcessingService, or a test collector).
//!
//! Stories: US-1.1 (batch), US-1.2 (manual trigger, run-lock).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::Result;
use crate::core::traits::IngestionApi;
use crate::core::types::{IngestReport, RawItem, SourceConfig, SourceKind};
use crate::ingestion::cursor_store::IngestionCursorStore;
use crate::ingestion::registry::ConnectorRegistry;

/// Where newly-collected items go. Implemented by ProcessingService (or a test
/// collector). Kept minimal so ingestion doesn't depend on processing types.
#[async_trait]
pub trait RawItemSink: Send + Sync {
    async fn accept(&self, items: Vec<RawItem>) -> Result<()>;
}

/// Sink that just buffers items (useful for manual pipelines / tests).
#[derive(Default)]
pub struct BufferSink {
    pub items: Mutex<Vec<RawItem>>,
}

#[async_trait]
impl RawItemSink for BufferSink {
    async fn accept(&self, items: Vec<RawItem>) -> Result<()> {
        self.items.lock().unwrap().extend(items);
        Ok(())
    }
}

pub struct IngestionService {
    registry: Arc<ConnectorRegistry>,
    cursors: Arc<IngestionCursorStore>,
    sink: Arc<dyn RawItemSink>,
    /// Sources currently running (single-flight lock, P6).
    running: Mutex<HashSet<SourceKind>>,
}

impl IngestionService {
    pub fn new(
        registry: Arc<ConnectorRegistry>,
        cursors: Arc<IngestionCursorStore>,
        sink: Arc<dyn RawItemSink>,
    ) -> Self {
        Self {
            registry,
            cursors,
            sink,
            running: Mutex::new(HashSet::new()),
        }
    }

    /// Try to acquire the per-source run lock. Returns false if already running.
    fn try_lock(&self, source: SourceKind) -> bool {
        self.running.lock().unwrap().insert(source)
    }

    fn unlock(&self, source: SourceKind) {
        self.running.lock().unwrap().remove(&source);
    }

    /// Run ingestion for a single source. Errors are contained here (P5) and
    /// reported via the returned report; they do not propagate to other sources.
    async fn run_one(&self, source: SourceKind, report: &mut IngestReport) {
        // Single-flight: if a run is in progress, report it as skipped-in-progress.
        if !self.try_lock(source) {
            report.skipped += 1; // US-1.2 AC2: already running.
            return;
        }

        let result = self.run_one_inner(source, report).await;
        if let Err(e) = result {
            // BR-I4 / US-1.1 AC3: one source failing must not stop the others.
            // The count alone leaves the owner with "1 error" and no way to act
            // on it, so the cause goes to the log AND the report so the UI can
            // surface *why* the source failed (bad token, network, …).
            eprintln!("[ingestion] {source:?} failed: {e}");
            report.errors += 1;
            report.error_messages.push(format!("{source:?}: {e}"));
        }
        self.unlock(source);
    }

    async fn run_one_inner(&self, source: SourceKind, report: &mut IngestReport) -> Result<()> {
        let Some(connector) = self.registry.get(source) else {
            return Ok(()); // not configured → nothing to do
        };

        let cursor = self.cursors.load_cursor(source).await?;
        let (items, next_cursor) = connector.sync(cursor).await?;

        let mut fresh = Vec::new();
        for it in items {
            if self.cursors.is_seen(source, &it.external_id).await? {
                report.skipped += 1; // idempotent skip (BR-I1)
                continue;
            }
            // Mark seen only after we commit to processing (BR-I2).
            self.cursors
                .mark_seen(source, &it.external_id, &Utc::now().to_rfc3339())
                .await?;
            report.collected += 1;
            fresh.push(it);
        }

        if !fresh.is_empty() {
            self.sink.accept(fresh).await?;
        }
        // Advance cursor only after a successful sync (BR-I3).
        self.cursors.save_cursor(source, &next_cursor).await?;
        report.remaining += connector.remaining(Some(next_cursor)).await.unwrap_or(0);
        Ok(())
    }
}

#[async_trait]
impl IngestionApi for IngestionService {
    async fn configure(&self, _source: SourceKind, _config: SourceConfig) -> Result<()> {
        // Connector construction (incl. SourceConfig) happens at wiring time in
        // this MVP; `configure` is a no-op placeholder kept for API completeness.
        // INTEGRATION-TODO: persist per-source config and rebuild the connector.
        Ok(())
    }

    async fn trigger(&self, source: Option<SourceKind>) -> Result<IngestReport> {
        let mut report = IngestReport::default();
        let targets = match source {
            Some(s) => vec![s],
            None => self.registry.sources(),
        };
        for src in targets {
            self.run_one(src, &mut report).await;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::Connector;
    use crate::core::types::Cursor;
    use crate::mocks::InMemoryStore;

    /// Deterministic connector returning a fixed set of items each sync.
    struct FakeConnector {
        source: SourceKind,
        items: Vec<RawItem>,
    }
    #[async_trait]
    impl Connector for FakeConnector {
        fn id(&self) -> SourceKind {
            self.source
        }
        async fn sync(&self, _c: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
            Ok((self.items.clone(), Cursor("c".into())))
        }
        fn supports_manual(&self) -> bool {
            true
        }
    }

    fn raw(id: &str) -> RawItem {
        RawItem {
            source: SourceKind::Session,
            external_id: id.into(),
            collected_at: Utc::now(),
            text: Some(format!("text {id}")),
            image_png: None,
        }
    }

    fn service_with(items: Vec<RawItem>) -> (IngestionService, Arc<BufferSink>) {
        let mut reg = ConnectorRegistry::new();
        reg.register(Arc::new(FakeConnector {
            source: SourceKind::Session,
            items,
        }));
        let store = Arc::new(InMemoryStore::default());
        let cursors = Arc::new(IngestionCursorStore::new(store));
        let sink = Arc::new(BufferSink::default());
        let svc = IngestionService::new(Arc::new(reg), cursors, sink.clone());
        (svc, sink)
    }

    #[tokio::test]
    async fn collects_then_idempotent_on_rerun() {
        let (svc, sink) = service_with(vec![raw("a"), raw("b")]);
        let r1 = svc.trigger(Some(SourceKind::Session)).await.unwrap();
        assert_eq!(r1.collected, 2);
        assert_eq!(sink.items.lock().unwrap().len(), 2);

        // Re-run: same items → nothing new (BR-I5, PBT-03 idempotency).
        let r2 = svc.trigger(Some(SourceKind::Session)).await.unwrap();
        assert_eq!(r2.collected, 0);
        assert_eq!(r2.skipped, 2);
        assert_eq!(sink.items.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn error_in_one_source_is_isolated() {
        struct Boom;
        #[async_trait]
        impl Connector for Boom {
            fn id(&self) -> SourceKind {
                SourceKind::Notion
            }
            async fn sync(&self, _c: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
                Err(crate::core::error::AppError::External("boom".into()))
            }
            fn supports_manual(&self) -> bool {
                false
            }
        }
        let mut reg = ConnectorRegistry::new();
        reg.register(Arc::new(FakeConnector {
            source: SourceKind::Session,
            items: vec![raw("a")],
        }));
        reg.register(Arc::new(Boom));
        let cursors = Arc::new(IngestionCursorStore::new(
            Arc::new(InMemoryStore::default()),
        ));
        let sink = Arc::new(BufferSink::default());
        let svc = IngestionService::new(Arc::new(reg), cursors, sink.clone());

        let r = svc.trigger(None).await.unwrap();
        // Session still collected despite Notion erroring.
        assert_eq!(r.collected, 1);
        assert_eq!(r.errors, 1);
        assert_eq!(sink.items.lock().unwrap().len(), 1);
        // The cause is reported so the UI can show *why* Notion failed.
        assert_eq!(r.error_messages.len(), 1);
        assert!(r.error_messages[0].contains("Notion"));
        assert!(r.error_messages[0].contains("boom"));
    }
}
