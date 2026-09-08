//! Routing: classify labels + thresholds → [`ProcessingDecision`] (US-2.1).
//!
//! BR-P2: noise/one-off → Drop; uncertain → Confirm (queue); needs-context →
//! Deepen (queue); certain → Store (fact). BR-P3: on missing labels, be
//! conservative and route to Confirm rather than dropping or auto-storing.

use crate::core::types::{FactCandidate, Provenance, RawItem, Scope};

/// What to do with a processed item.
#[derive(Clone, Debug)]
pub enum ProcessingDecision {
    /// Certain → store directly as a confirmed fact.
    Store(FactCandidate),
    /// Uncertain → enqueue a confirm-style interview item.
    Confirm(FactCandidate),
    /// Needs more context → enqueue a deepen-style interview item.
    Deepen {
        question: String,
        hypothesis: Option<String>,
    },
    /// Noise / one-off → filtered out (not stored).
    Drop { reason: String },
}

fn has(labels: &[String], needle: &str) -> bool {
    labels.iter().any(|l| l.eq_ignore_ascii_case(needle))
}

fn scope_from_labels(labels: &[String]) -> Scope {
    if has(labels, "company") {
        Scope::Company
    } else if has(labels, "personal") {
        Scope::Personal
    } else {
        Scope::Unknown
    }
}

/// Decide routing from classification labels + the derived summary.
///
/// `summary` becomes the candidate body; `raw` provides provenance.
pub fn route(labels: &[String], summary: &str, raw: &RawItem) -> ProcessingDecision {
    let provenance = Provenance {
        source: raw.source,
        collected_at: raw.collected_at,
    };
    let candidate = |scope: Scope| FactCandidate {
        title: title_of(summary),
        body: summary.to_string(),
        provenance: provenance.clone(),
        suggested_scope: scope,
    };

    // Noise / one-off → drop (US-2.1 AC2).
    if has(labels, "noise") || has(labels, "one-off") {
        return ProcessingDecision::Drop {
            reason: "classified as noise/one-off".into(),
        };
    }
    // Needs-context → deepen (US-2.1 AC3).
    if has(labels, "needs-context") {
        return ProcessingDecision::Deepen {
            question: format!("More context needed about: {}", title_of(summary)),
            hypothesis: Some(summary.to_string()),
        };
    }
    // Uncertain → confirm (US-2.1 AC3).
    if has(labels, "uncertain") {
        return ProcessingDecision::Confirm(candidate(scope_from_labels(labels)));
    }
    // Missing/empty labels → conservative confirm (BR-P3), never silent drop.
    if labels.is_empty() {
        return ProcessingDecision::Confirm(candidate(Scope::Unknown));
    }
    // Otherwise certain → store (US-2.1 AC1).
    ProcessingDecision::Store(candidate(scope_from_labels(labels)))
}

/// Derive a short title from the summary (first line / clipped).
fn title_of(summary: &str) -> String {
    let first = summary.lines().next().unwrap_or(summary).trim();
    const MAX: usize = 80;
    if first.len() <= MAX {
        first.to_string()
    } else {
        let mut end = MAX;
        while !first.is_char_boundary(end) {
            end -= 1;
        }
        first[..end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::core::types::SourceKind;

    fn raw() -> RawItem {
        RawItem {
            source: SourceKind::Session,
            external_id: "x".into(),
            collected_at: Utc::now(),
            text: Some("t".into()),
            image_png: None,
        }
    }

    #[test]
    fn noise_is_dropped() {
        let d = route(&["noise".into()], "junk", &raw());
        assert!(matches!(d, ProcessingDecision::Drop { .. }));
    }

    #[test]
    fn uncertain_is_confirm() {
        let d = route(&["uncertain".into(), "personal".into()], "maybe uses vim", &raw());
        match d {
            ProcessingDecision::Confirm(c) => assert_eq!(c.suggested_scope, Scope::Personal),
            _ => panic!("expected confirm"),
        }
    }

    #[test]
    fn needs_context_is_deepen() {
        let d = route(&["needs-context".into()], "some topic", &raw());
        assert!(matches!(d, ProcessingDecision::Deepen { .. }));
    }

    #[test]
    fn empty_labels_are_conservative_confirm() {
        let d = route(&[], "unlabeled", &raw());
        assert!(matches!(d, ProcessingDecision::Confirm(_)));
    }

    #[test]
    fn certain_company_is_store() {
        let d = route(&["company".into()], "deploy with make deploy", &raw());
        match d {
            ProcessingDecision::Store(c) => assert_eq!(c.suggested_scope, Scope::Company),
            _ => panic!("expected store"),
        }
    }
}
