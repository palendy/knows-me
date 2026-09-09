//! Routing: classify labels + thresholds → [`ProcessingDecision`] (US-2.1).
//!
//! BR-P2: noise/one-off → Drop; uncertain → Confirm (queue); needs-context →
//! Deepen (queue); certain → Store (fact). BR-P3: on missing labels, be
//! conservative and route to Confirm rather than dropping or auto-storing.

use crate::core::types::{
    normalize_topic, FactCandidate, FactKind, Provenance, RawItem, Scope, Visibility,
};

/// Labels the router interprets as control words. Everything else the
/// classifier returned is a topic.
const CONTROL_LABELS: &[&str] = &[
    "certain",
    "uncertain",
    "needs-context",
    "noise",
    "one-off",
    "company",
    "personal",
    "unknown",
    "practice",
    "preference",
    "project",
    "concern",
    "public",
    "private",
    "unclear",
];

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

/// Everything that is not a control word, normalized and de-duplicated.
///
/// The classifier has always returned these; until now they were computed and
/// thrown away, which is why nothing could be aggregated across sessions.
fn topics_from_labels(labels: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for label in labels {
        if CONTROL_LABELS.contains(&label.as_str()) {
            continue;
        }
        if let Some(t) = normalize_topic(label) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

fn kind_from_labels(labels: &[String]) -> FactKind {
    if has(labels, "concern") {
        FactKind::Concern
    } else if has(labels, "preference") {
        FactKind::Preference
    } else if has(labels, "practice") {
        FactKind::Practice
    } else if has(labels, "project") {
        FactKind::Project
    } else {
        FactKind::Note
    }
}

/// Only an explicit `public` shares. Everything else — `private`, `unclear`, or
/// a classifier that said nothing — stays private, so a missing label can never
/// become a leak.
fn visibility_from_labels(labels: &[String]) -> Visibility {
    if has(labels, "public") {
        Visibility::Shared
    } else {
        Visibility::Private
    }
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
    let topics = topics_from_labels(labels);
    let kind = kind_from_labels(labels);
    let candidate = |scope: Scope| FactCandidate {
        title: title_of(summary),
        body: body_of(summary),
        provenance: provenance.clone(),
        suggested_scope: scope,
        topics: topics.clone(),
        kind,
        visibility: visibility_from_labels(labels),
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
    // Sharing is a decision the owner makes, so an item the classifier could
    // not place goes to the queue rather than being filed silently.
    if has(labels, "unclear") {
        return ProcessingDecision::Confirm(candidate(scope_from_labels(labels)));
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

/// Index of the line the summarizer meant as the title.
///
/// Shared by [`title_of`] and [`body_of`] so the two can never disagree about
/// which line the title is.
fn title_line_index(summary: &str) -> Option<usize> {
    summary
        .lines()
        .position(|l| !l.trim().is_empty() && !is_bare_url(l.trim()))
}

/// The summary minus its title line.
///
/// Comparing against the *rendered* title would miss long ones — `title_of`
/// clips at 80 characters, so a longer title no longer equals the line it came
/// from and the page ends up repeating its own heading. Dropping by position
/// avoids the question.
fn body_of(summary: &str) -> String {
    let body = match title_line_index(summary) {
        Some(i) => summary
            .lines()
            .skip(i + 1)
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        None => String::new(),
    };

    // A one-line summary is its own body rather than nothing.
    if body.is_empty() {
        summary.trim().to_string()
    } else {
        body
    }
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
    let first = title_line_index(summary)
        .and_then(|i| summary.lines().nth(i))
        .map(str::trim)
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

    fn stored(labels: &[&str]) -> FactCandidate {
        let labels: Vec<String> = labels.iter().map(|l| l.to_string()).collect();
        match route(&labels, "제목\n본문", &raw()) {
            ProcessingDecision::Store(c) => c,
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn only_an_explicit_public_label_shares() {
        assert_eq!(
            stored(&["certain", "public"]).visibility,
            Visibility::Shared
        );
        // Everything else stays private — including a classifier that said
        // nothing about visibility at all.
        for labels in [
            vec!["certain", "private"],
            vec!["certain"],
            vec!["certain", "company"],
        ] {
            assert_eq!(
                stored(&labels).visibility,
                Visibility::Private,
                "labels {labels:?} must not share"
            );
        }
    }

    #[test]
    fn an_unclear_visibility_becomes_a_question_rather_than_a_filing() {
        // Sharing is the owner's call; an item the classifier could not place
        // must not be filed silently either way.
        let decision = route(&["certain".into(), "unclear".into()], "제목\n본문", &raw());
        match decision {
            ProcessingDecision::Confirm(c) => {
                assert_eq!(c.visibility, Visibility::Private, "private until answered")
            }
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn a_long_title_is_still_stripped_from_the_body() {
        // `title_of` clips at 80 chars, so the rendered title no longer equals
        // the line it came from — the page used to repeat its own heading.
        let long =
            "실제 Excel 환경 미검증으로 인한 상용 수준 Add-in 완료 기준 미달과 반복 판정 문제";
        let summary = format!("{long}\n\n헤드리스 테스트는 통과했으나 실기 환경 검증이 없다.");

        match route(&["certain".into()], &summary, &raw()) {
            ProcessingDecision::Store(c) => {
                assert!(
                    !c.body.contains("Add-in 완료 기준 미달과"),
                    "body: {}",
                    c.body
                );
                assert!(c.body.starts_with("헤드리스"));
            }
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn a_single_line_summary_is_its_own_body() {
        match route(&["certain".into()], "한 줄짜리 사실", &raw()) {
            ProcessingDecision::Store(c) => assert_eq!(c.body, "한 줄짜리 사실"),
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn classification_topics_reach_the_candidate() {
        match route(
            &[
                "certain".into(),
                "company".into(),
                "concern".into(),
                "Deployment".into(),
            ],
            "제목\n본문",
            &raw(),
        ) {
            ProcessingDecision::Store(c) => {
                assert_eq!(
                    c.topics,
                    vec!["deployment"],
                    "control words must not become topics"
                );
                assert_eq!(c.kind, FactKind::Concern);
            }
            other => panic!("expected Store, got {other:?}"),
        }
    }

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
