//! U2 — Processing. Masking gateway, summarize/classify, routing to Knowledge
//! (U3) / Interview (U3), transfer transparency log, and offline pending queue.
//!
//! Layout:
//! - [`transfer_log`] — append-only transparency log (US-2.3)
//! - [`llm_gateway`] — security boundary: mask → log → LlmClient, with retry (US-2.2)
//! - [`router`] — classify labels → ProcessingDecision (US-2.1)
//! - [`pending`] — offline-degradation queue (NFR-3)
//! - [`service`] — [`ProcessingService`] implementing [`crate::core::traits::ProcessingApi`]

pub mod llm_gateway;
pub mod pending;
pub mod router;
pub mod service;
pub mod transfer_log;

pub use llm_gateway::LlmGateway;
pub use pending::PendingQueue;
pub use router::{route, ProcessingDecision};
pub use service::{ProcessingService, ProcessingSink};
pub use transfer_log::{TransferLog, TransferLogEntry, TransferOp};
