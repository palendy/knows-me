//! knows-me core — U1 (Core Platform & Security)
//!
//! Milestone 0 deliverable: the **shared contract layer** that unblocks parallel
//! development of U2 (Ingestion & Processing), U3 (Knowledge & Interview) and
//! U4 (Interface & Persona).
//!
//! - [`core::types`] — shared domain types (Fact, QueueItem, DTOs, SourceKind, ...)
//! - [`core::traits`] — service interfaces every unit codes against
//! - [`core::error`] — unified error type
//! - [`mocks`] — in-memory mock implementations so other units can build/test today
//!
//! The real implementations (crypto Vault, file-based FactStore, connectors, LLM
//! client, Tauri commands) land in U1 full implementation and the per-unit
//! CONSTRUCTION stages.

pub mod core;
pub mod mocks;

pub use core::error::{AppError, Result};
