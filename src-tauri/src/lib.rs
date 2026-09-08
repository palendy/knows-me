//! knows-me core — U1 (Core Platform & Security)
//!
//! The platform + security core, plus the shared contract layer that unblocks
//! parallel development of U2 (Ingestion & Processing), U3 (Knowledge &
//! Interview) and U4 (Interface & Persona).
//!
//! Shared contracts (Milestone 0):
//! - [`core::types`] — shared domain types (Fact, QueueItem, DTOs, SourceKind, ...)
//! - [`core::traits`] — service interfaces every unit codes against
//! - [`core::error`] — unified error type
//! - [`mocks`] — in-memory mock implementations so other units can build/test
//!
//! U1 implementation:
//! - [`security`] — password KDF (Argon2id), AES-256-GCM vault, encrypted store
//! - [`llm`] — masking gateway, prompts, cloud client, transfer-transparency log
//! - [`core::app_state`] / [`core::scheduler`] / [`core::commands`] — platform
//!   wiring, periodic batch runner, and the front-end-facing command layer
//!
//! The Tauri desktop shell (windows + `#[tauri::command]` handlers) lives in the
//! sibling `desktop/` crate; the React frontend lives in `../src/`.

pub mod core;
pub mod interview;
pub mod knowledge;
pub mod llm;
pub mod mocks;
pub mod security;

// U2 — Ingestion & Processing (Dev B). Implemented against the U1 contracts in
// `core::traits`, using U1/U3 mocks (`mocks`) during parallel development.
pub mod ingestion;
pub mod processing;

pub use core::app_state::AppState;
pub use core::error::{AppError, Result};
pub use core::scheduler::Scheduler;
