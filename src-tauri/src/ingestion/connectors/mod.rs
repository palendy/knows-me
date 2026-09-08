//! Source connectors. Each implements U1's [`crate::core::traits::Connector`].
//!
//! Full implementations: [`session`], [`file`] (text/markdown).
//! Skeletons (INTEGRATION-TODO): [`notion`], [`gmail`], and the PDF/DOCX/vision
//! branches inside [`file`] — they satisfy the contract and build offline, but
//! the real external calls / heavy parsers are wired up during integration.

pub mod file;
pub mod gmail;
pub mod notion;
pub mod session;

pub use file::FileConnector;
pub use gmail::GmailConnector;
pub use notion::NotionConnector;
pub use session::SessionConnector;
