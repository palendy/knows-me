//! U3 — Knowledge module.
//!
//! The file-based "LLM wiki": confirmed facts stored as JSON via U1's
//! [`EncryptedStore`], an append-only change history, and an in-memory index
//! (inverted postings + lightweight metadata cache) that powers search, graph,
//! and dashboard reads without touching disk.
//!
//! - [`fact_store::FactStore`] — encrypted fact documents (`ns=facts`)
//! - [`history::HistoryTracker`] — append-only change log (`ns=fact_history`)
//! - [`search_index::SearchIndex`] — in-memory, derived, rebuilt on unlock
//! - [`service::KnowledgeService`] — implements [`crate::core::traits::KnowledgeApi`]

pub mod fact_store;
pub mod history;
pub mod search_index;
pub mod service;

pub use service::KnowledgeService;
