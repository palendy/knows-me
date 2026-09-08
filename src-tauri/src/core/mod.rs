//! Shared contract layer (types, traits, errors) — the single source of truth
//! that all units (U1–U4) depend on.

pub mod error;
pub mod traits;
pub mod types;

pub use error::{AppError, Result};
