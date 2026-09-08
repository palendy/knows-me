//! U2 — Ingestion. Pluggable source connectors + incremental/idempotent
//! collection orchestration.
//!
//! Layout:
//! - [`cursor_store`] — idempotency gate (seen keys) + per-source cursors
//! - [`registry`] — `SourceKind -> Connector` plugin registry
//! - [`connectors`] — Session/File (full) + Notion/Gmail (skeleton, INTEGRATION-TODO)
//! - [`service`] — [`IngestionService`] implementing [`crate::core::traits::IngestionApi`]

pub mod connectors;
pub mod cursor_store;
pub mod registry;
pub mod service;

pub use cursor_store::IngestionCursorStore;
pub use registry::ConnectorRegistry;
pub use service::IngestionService;
