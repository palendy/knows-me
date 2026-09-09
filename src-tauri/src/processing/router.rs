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

    // The summarizer itself found nothing durable — believe it over the
    // classifier, which only ever saw the same input.
    if crate::llm::prompts::is_nothing(summary) {
        return ProcessingDecision::Drop {
            reason: "no durable fact in item".into(),
        };
    }
    // Noise / one-off (US-2.1 AC2). The two models disagree here: the
    // summarizer was asked "is there a durable fact?" and produced one, while
    // the classifier calls the item noise. Dropping on that disagreement is how
    // real context goes missing without a trace, so it becomes a question for
    // the owner instead — which is what the interview queue is for (BR-P3).
    if has(labels, "noise") || has(labels, "one-off") {
        return ProcessingDecision::Confirm(candidate(scope_from_labels(labels)));
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

/// Whether a line is just a link, with no words around it.
fn is_bare_url(line: &str) -> bool {
    let mut tokens = line.split_whitespace();
    match (tokens.next(), tokens.next()) {
        (Some(single), None) => single.starts_with("http://") || single.starts_with("https://"),
        _ => false,
    }
}

/// Derive a short title from the summary (first line / clipped).
fn title_of(summary: &str) -> String {
    // The first line is the title the summarizer was asked for, unless it
    // handed back something that names nothing — a bare URL is the common
    // case, and "https://github.com/…/README" as a fact title is noise in the
    // wiki. Fall through to the first line that reads like prose.
    let first = summary
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !is_bare_url(l))
        .unwrap_or_else(|| summary.trim());
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

    #[test]
    fn a_bare_url_is_not_a_fact_title() {
        let summary = "https://github.com/someone/repo/blob/main/README\n\
                       사용자는 한국어 번역 문서를 참고한다";
        let decision = route(&["certain".into()], summary, &raw());
        match decision {
            ProcessingDecision::Store(c) => {
                assert!(!c.title.starts_with("http"), "got title: {}", c.title);
                assert!(c.title.contains("한국어"));
            }
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn a_nothing_summary_is_dropped_whatever_the_labels_say() {
        let decision = route(&["certain".into(), "personal".into()], "NOTHING", &raw());
        assert!(matches!(decision, ProcessingDecision::Drop { .. }));
    }
    use crate::core::types::SourceKind;
    use chrono::Utc;

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
    fn disagreement_between_the_models_becomes_a_question_not_a_deletion() {
        // The summarizer found a fact; the classifier called it noise. Asking
        // costs the owner one queue item; dropping costs them the fact.
        let decision = route(
            &["noise".into()],
            "사용자는 배포에 make deploy를 쓴다",
            &raw(),
        );
        assert!(matches!(decision, ProcessingDecision::Confirm(_)));
    }

    #[test]
    fn only_the_summarizer_can_authorize_a_drop() {
        let decision = route(&["noise".into()], "NOTHING", &raw());
        assert!(matches!(decision, ProcessingDecision::Drop { .. }));
    }

    #[test]
    fn uncertain_is_confirm() {
        let d = route(
            &["uncertain".into(), "personal".into()],
            "maybe uses vim",
            &raw(),
        );
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
