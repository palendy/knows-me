//! U1 shared LLM gateway: masking + prompt construction + cloud client + the
//! transfer-transparency log. Every path to the cloud passes through here.
//!
//! - [`masker::RegexMasker`] — strip/restore identifiers (US-2.2, R1).
//! - [`prompts`] — pure prompt builders / response parsers.
//! - [`transfer_log::TransferLog`] — records what left the device (NFR-2).
//! - [`client`] — real Anthropic client under the `llm-http` feature.

pub mod client;
pub mod masker;
pub mod prompts;
pub mod transfer_log;

pub use masker::RegexMasker;
pub use transfer_log::TransferLog;
