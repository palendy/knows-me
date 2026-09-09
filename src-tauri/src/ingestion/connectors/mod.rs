//! Source connectors. Each implements U1's [`crate::core::traits::Connector`].
//!
//! Full implementations: [`session`], [`file`] (text/markdown).
//! Skeletons (INTEGRATION-TODO): [`notion`], [`gmail`], and the PDF/DOCX/vision
//! branches inside [`file`] — they satisfy the contract and build offline, but
//! the real external calls / heavy parsers are wired up during integration.

pub mod file;
pub mod gmail;
/// Local dev-only fake Gmail data (debug builds only). See [`gmail`].
#[cfg(debug_assertions)]
pub mod gmail_fixtures;
pub mod notion;
pub mod session;
pub mod spec;

pub use file::FileConnector;
pub use gmail::GmailConnector;
pub use notion::NotionConnector;
pub use session::{SessionConnector, SessionProject};
pub use spec::{credential_satisfies, credential_spec, FieldSpec};
