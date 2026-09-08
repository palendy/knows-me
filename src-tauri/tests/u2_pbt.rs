//! U2 property-based tests (PBT-02/03/07/08, Partial mode — blocking).
//!
//! Framework: `proptest` (PBT-09). Supports custom strategies, automatic
//! shrinking, and seed-based reproducibility. On failure, proptest writes a
//! regression seed to `src-tauri/tests/u2_pbt.proptest-regressions` so the exact
//! failing case is replayable (PBT-08).
//!
//! Properties covered:
//! - PBT-02 round-trip: `unmask(mask(x)) == x` for the masker contract.
//! - PBT-03 invariants:
//!     * ingestion idempotency: re-running collection yields collected == 0.
//!     * cursor-store `seen` monotonicity.
//! - PBT-07 generators: domain strategies for identifier-bearing text and for
//!   sequences of `RawItem`s.

use std::sync::Arc;

use knows_me_core::core::traits::{Connector, IngestionApi, Masker};
use knows_me_core::core::types::{Cursor, RawItem, SourceKind};
use knows_me_core::ingestion::service::BufferSink;
use knows_me_core::ingestion::{ConnectorRegistry, IngestionCursorStore, IngestionService};
use knows_me_core::mocks::{InMemoryStore, NoopMasker};

use async_trait::async_trait;
use proptest::prelude::*;

// --- PBT-07: domain generators ---------------------------------------------

/// Text that may embed identifier-like tokens (emails, phone-ish digits, names).
/// Used to exercise masking round-trip and (once a rule-based masker exists) the
/// "no original identifier remains" invariant.
fn identifier_text() -> impl Strategy<Value = String> {
    let name = "[A-Z][a-z]{1,8}";
    let email = "[a-z]{3,8}@[a-z]{3,6}\\.(com|org)";
    let phone = "[0-9]{3}-[0-9]{4}-[0-9]{4}";
    prop::collection::vec(
        prop_oneof![
            name.prop_map(|s| s),
            email.prop_map(|s| s),
            phone.prop_map(|s| s),
            "[a-z ]{1,20}".prop_map(|s| s),
        ],
        0..6,
    )
    .prop_map(|parts| parts.join(" "))
}

/// A sequence of RawItems for one source, with (possibly repeated) external ids.
fn raw_items() -> impl Strategy<Value = Vec<RawItem>> {
    prop::collection::vec(
        ("[a-z0-9]{1,6}", identifier_text()).prop_map(|(id, text)| RawItem {
            source: SourceKind::Session,
            external_id: id,
            // Fixed timestamp: proptest must be deterministic; parse a constant.
            collected_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            text: Some(text),
            image_png: None,
        }),
        0..8,
    )
}

// --- PBT-02: masking round-trip --------------------------------------------

proptest! {
    #[test]
    fn masker_roundtrip(text in identifier_text()) {
        // Contract property: unmask(mask(x)) == x. Verified here against the
        // NoopMasker; the real U1 masker must satisfy the same property (shared
        // contract test).
        let m = NoopMasker;
        let (masked, map) = m.mask(&text);
        let restored = m.unmask(&masked, &map);
        prop_assert_eq!(restored, text);
    }
}

// --- PBT-03: ingestion idempotency -----------------------------------------

/// Connector that always returns the same fixed batch.
struct FixedConnector {
    items: Vec<RawItem>,
}
#[async_trait]
impl Connector for FixedConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Session
    }
    async fn sync(&self, _c: Option<Cursor>) -> knows_me_core::Result<(Vec<RawItem>, Cursor)> {
        Ok((self.items.clone(), Cursor("c".into())))
    }
    fn supports_manual(&self) -> bool {
        true
    }
}

fn build_service(items: Vec<RawItem>) -> (IngestionService, Arc<BufferSink>) {
    let mut reg = ConnectorRegistry::new();
    reg.register(Arc::new(FixedConnector { items }));
    let cursors = Arc::new(IngestionCursorStore::new(
        Arc::new(InMemoryStore::default()),
    ));
    let sink = Arc::new(BufferSink::default());
    let svc = IngestionService::new(Arc::new(reg), cursors, sink.clone());
    (svc, sink)
}

proptest! {
    #[test]
    fn ingestion_is_idempotent(items in raw_items()) {
        // Property: after the first trigger collects the unique items, a second
        // trigger with the same source collects nothing new (BR-I5 / NFR-5).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let unique: std::collections::HashSet<&str> =
                items.iter().map(|i| i.external_id.as_str()).collect();
            let unique_count = unique.len();

            let (svc, sink) = build_service(items);
            let r1 = svc.trigger(Some(SourceKind::Session)).await.unwrap();
            prop_assert_eq!(r1.collected, unique_count);

            let r2 = svc.trigger(Some(SourceKind::Session)).await.unwrap();
            prop_assert_eq!(r2.collected, 0);
            // Sink only ever received the unique items.
            prop_assert_eq!(sink.items.lock().unwrap().len(), unique_count);
            Ok(())
        })?;
    }
}

// --- PBT-03: seen monotonicity ---------------------------------------------

proptest! {
    #[test]
    fn seen_is_monotonic(ids in prop::collection::vec("[a-z0-9]{1,6}", 0..10)) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let cs = IngestionCursorStore::new(Arc::new(InMemoryStore::default()));
            for id in &ids {
                cs.mark_seen(SourceKind::Session, id, "t").await.unwrap();
            }
            // Every marked id stays seen (monotonic — never un-sets).
            for id in &ids {
                prop_assert!(cs.is_seen(SourceKind::Session, id).await.unwrap());
            }
            Ok(())
        })?;
    }
}
