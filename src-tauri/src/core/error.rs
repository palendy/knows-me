//! Unified error type shared across all units.

use thiserror::Error;

/// Application-wide error. Units return `Result<T>` (alias below) from all
/// fallible contract methods.
#[derive(Debug, Error)]
pub enum AppError {
    /// The vault is locked; the user must unlock (enter the password) first.
    #[error("locked: unlock required")]
    Locked,

    /// A requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Input failed validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Encryption / decryption / key-derivation failure.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Local filesystem / storage failure.
    #[error("io error: {0}")]
    Io(String),

    /// An external service (Notion/Gmail/LLM) failed.
    #[error("external service error: {0}")]
    External(String),

    /// (De)serialization failure.
    #[error("serialization error: {0}")]
    Serde(String),
}

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, AppError>;
