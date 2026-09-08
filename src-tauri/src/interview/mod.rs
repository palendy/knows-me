//! U3 — Interview module.
//!
//! The interview queue (confirm / deepen items) and answer intake. Items are
//! persisted as JSON via U1's [`EncryptedStore`] (`ns=queue`); answering an item
//! turns it into a confirmed fact (via [`crate::knowledge`]) and, for deepen
//! items, derives bounded follow-up questions using U1's `Masker` + `LlmClient`.
//!
//! - [`queue_manager::QueueManager`] — encrypted queue CRUD + pure policy helpers
//! - [`answer_intake`] — answer dispatch + follow-up generation
//! - [`service::InterviewService`] — implements [`crate::core::traits::InterviewApi`]

pub mod answer_intake;
pub mod queue_manager;
pub mod service;

pub use service::InterviewService;
