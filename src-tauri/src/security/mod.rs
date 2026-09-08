//! U1 Security: password-based key management and the encrypted store.
//!
//! - [`key_manager::PasswordKeyManager`] — Argon2id KDF, lock/unlock, in-memory key.
//! - [`vault`] — AES-256-GCM authenticated encryption primitives.
//! - [`store::FileEncryptedStore`] — encrypted-at-rest key/value + credential store.

pub mod key_manager;
pub mod store;
pub mod vault;

pub use key_manager::PasswordKeyManager;
pub use store::FileEncryptedStore;
