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
use crate::core::traits::{IngestionApi, ProgressReporter};
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
    async fn run_one(
        &self,
        source: SourceKind,
        report: &mut IngestReport,
        progress: &dyn ProgressReporter,
    ) {
        // Single-flight: if a run is in progress, report it as skipped-in-progress.
        if !self.try_lock(source) {
            report.skipped += 1; // US-1.2 AC2: already running.
            return;
        }

        let result = self.run_one_inner(source, report, progress).await;
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

    async fn run_one_inner(
        &self,
        source: SourceKind,
        report: &mut IngestReport,
        progress: &dyn ProgressReporter,
    ) -> Result<bool> {
        let Some(connector) = self.registry.get(source) else {
            return Ok(false); // not configured → nothing to do
        };

        let cursor = self.cursors.load_cursor(source).await?;
        let before = cursor.clone();
        let (items, next_cursor) = connector.sync(cursor, progress).await?;

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
        report.remaining += connector
            .remaining(Some(next_cursor.clone()))
            .await
            .unwrap_or(0);

        // Whether this pass moved the frontier at all. A pass can legitimately
        // yield no *items* while still advancing (a tooling-only transcript is
        // covered but carries nothing to learn from), so emptiness is the wrong
        // stop condition — a standstill cursor is the right one.
        Ok(before.as_ref() != Some(&next_cursor))
    }

    /// Repeat [`Self::run_one_inner`] until the source stops advancing.
    ///
    /// Connectors cap one pass (`MAX_FILES_PER_SYNC` for sessions) so a single
    /// press of "collect" stays bounded. That cap silently became a ceiling on
    /// what the vault could ever hold: with 431 transcripts on disk and 30 per
    /// pass, everything past the newest 30 was only reachable by pressing the
    /// button fifteen times, and a caller driving the connector directly with a
    /// null cursor got the same newest 30 on every run — forever.
    ///
    /// `max_passes` bounds the work so a pathological connector cannot spin.
    async fn run_until_done(
        &self,
        source: SourceKind,
        report: &mut IngestReport,
        progress: &dyn ProgressReporter,
        max_passes: usize,
    ) {
        for _ in 0..max_passes {
            match self.run_one_inner(source, report, progress).await {
                Ok(true) => continue,
                Ok(false) => return,
                Err(e) => {
                    report.errors += 1;
                    eprintln!("[ingest] {source:?}: {e}");
                    report.error_messages.push(format!("{source:?}: {e}"));
                    return;
                }
            }
        }
    }
}

#[async_trait]
impl IngestionApi for IngestionService {
    async fn configure(&self, source: SourceKind, config: SourceConfig) -> Result<()> {
        // Applies to the live connector rather than rebuilding the registry:
        // connectors are handed out as `Arc<dyn Connector>` and an owner
        // re-scoping collection expects the next press to honour the change.
        // Persisting the choice across restarts is the caller's job — the
        // desktop shell stores it in the encrypted vault and replays it on
        // unlock, so this layer keeps no config of its own.
        let Some(connector) = self.registry.get(source) else {
            return Ok(());
        };
        connector.configure(&config)
    }

    async fn trigger(
        &self,
        source: Option<SourceKind>,
        progress: &dyn ProgressReporter,
    ) -> Result<IngestReport> {
        let mut report = IngestReport::default();
        let targets = match source {
            Some(s) => vec![s],
            None => self.registry.sources(),
        };
        for src in targets {
            self.run_one(src, &mut report, progress).await;
        }
        Ok(report)
    }

    async fn trigger_all(
        &self,
        source: Option<SourceKind>,
        progress: &dyn ProgressReporter,
        max_passes: usize,
    ) -> Result<IngestReport> {
        let mut report = IngestReport::default();
        let targets = match source {
            Some(s) => vec![s],
            None => self.registry.sources(),
        };
        for src in targets {
            // Same single-flight guard as `trigger`: a long multi-pass run
            // must not overlap with a button press.
            if !self.try_lock(src) {
                report.skipped += 1;
                continue;
            }
            // `remaining` accumulates per pass; only the final pass's figure
            // describes what is actually left.
            report.remaining = 0;
            self.run_until_done(src, &mut report, progress, max_passes)
                .await;
            self.unlock(src);
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::{Connector, NoProgress};
    use crate::core::types::Cursor;
    use crate::mocks::InMemoryStore;

    /// A connector with a real backlog: hands out `batch` ids per sync and
    /// advances its cursor, exactly like the session connector's file cap.
    struct BatchedConnector {
        total: usize,
        batch: usize,
    }
    #[async_trait]
    impl Connector for BatchedConnector {
        fn id(&self) -> SourceKind {
            SourceKind::Session
        }
        async fn sync(
            &self,
            c: Option<Cursor>,
            _p: &dyn ProgressReporter,
        ) -> Result<(Vec<RawItem>, Cursor)> {
            let done: usize = c.and_then(|c| c.0.parse().ok()).unwrap_or(0);
            let take = self.batch.min(self.total.saturating_sub(done));
            let items = (done..done + take)
                .map(|i| RawItem {
                    source: SourceKind::Session,
                    external_id: format!("item-{i}"),
                    collected_at: Utc::now(),
                    text: Some(format!("body {i}")),
                    image_png: None,
                })
                .collect();
            Ok((items, Cursor((done + take).to_string())))
        }
        fn supports_manual(&self) -> bool {
            true
        }
    }

    fn batched_service(total: usize, batch: usize) -> IngestionService {
        let mut registry = ConnectorRegistry::new();
        registry.register(Arc::new(BatchedConnector { total, batch }));
        let store = Arc::new(InMemoryStore::default());
        IngestionService::new(
            Arc::new(registry),
            Arc::new(IngestionCursorStore::new(store.clone())),
            Arc::new(BufferSink::default()),
        )
    }

    #[tokio::test]
    async fn one_trigger_takes_only_one_batch() {
        let svc = batched_service(95, 30);
        let report = svc
            .trigger(Some(SourceKind::Session), &NoProgress)
            .await
            .unwrap();
        assert_eq!(report.collected, 30, "a single press must stay bounded");
    }

    #[tokio::test]
    async fn trigger_all_drains_the_whole_backlog() {
        // The defect this exists to prevent: with a 30-item cap and 95 items on
        // disk, everything past the newest 30 was unreachable — one press took
        // 30 and a caller driving the connector directly got the same 30 every
        // time. 431 real transcripts sat behind that ceiling.
        let svc = batched_service(95, 30);
        let report = svc
            .trigger_all(Some(SourceKind::Session), &NoProgress, 100)
            .await
            .unwrap();
        assert_eq!(report.collected, 95, "every item must be reached");
        assert_eq!(report.remaining, 0);
    }

    #[tokio::test]
    async fn trigger_all_stops_when_the_cursor_stops_moving() {
        // An exhausted source returns no items and an unchanged cursor. Without
        // a standstill check this spins to `max_passes` on every run.
        let svc = batched_service(10, 30);
        let report = svc
            .trigger_all(Some(SourceKind::Session), &NoProgress, 100)
            .await
            .unwrap();
        assert_eq!(report.collected, 10);

        // A second run has nothing to do and must not re-collect.
        let again = svc
            .trigger_all(Some(SourceKind::Session), &NoProgress, 100)
            .await
            .unwrap();
        assert_eq!(again.collected, 0, "already-seen items must not come back");
    }

    #[tokio::test]
    async fn max_passes_bounds_a_connector_that_never_finishes() {
        let svc = batched_service(usize::MAX, 1);
        let report = svc
            .trigger_all(Some(SourceKind::Session), &NoProgress, 5)
            .await
            .unwrap();
        assert_eq!(report.collected, 5, "the bound must hold");
    }

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
        async fn sync(
            &self,
            _c: Option<Cursor>,
            _p: &dyn ProgressReporter,
        ) -> Result<(Vec<RawItem>, Cursor)> {
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
        let r1 = svc
            .trigger(Some(SourceKind::Session), &NoProgress)
            .await
            .unwrap();
        assert_eq!(r1.collected, 2);
        assert_eq!(sink.items.lock().unwrap().len(), 2);

        // Re-run: same items → nothing new (BR-I5, PBT-03 idempotency).
        let r2 = svc
            .trigger(Some(SourceKind::Session), &NoProgress)
            .await
            .unwrap();
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
            async fn sync(
                &self,
                _c: Option<Cursor>,
                _p: &dyn ProgressReporter,
            ) -> Result<(Vec<RawItem>, Cursor)> {
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

        let r = svc.trigger(None, &NoProgress).await.unwrap();
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
