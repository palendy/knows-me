//! Shared contract layer (types, traits, errors) — the single source of truth
//! that all units (U1–U4) depend on.

pub mod app_state;
pub mod commands;
pub mod error;
pub mod scheduler;
pub mod text;
pub mod traits;
pub mod types;

pub use app_state::AppState;
pub use error::{AppError, Result};
pub use scheduler::Scheduler;
