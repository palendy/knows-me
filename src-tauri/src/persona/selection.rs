//! Mini-home highlight selection (US-5.2) — pure and deterministic (BR-V2).

use std::collections::HashMap;

use crate::core::types::{Fact, FactId, FactSummary};

/// Default highlight count: a 3x3 mini-home grid.
pub const DEFAULT_HIGHLIGHTS: usize = 9;

/// Rank already-summarised facts by how representative they are.
///
/// Ordering is `(link degree desc, title asc, id asc)` — a total order, so the
/// result does not depend on input order (BR-V2/BR-V4).
///
/// The recency tiebreaker the rule originally carried is gone on purpose:
/// [`FactSummary`] has no timestamp, and fetching every fact just to read one
/// would reintroduce the N+1 this function exists to avoid (U4-NFR-P2). If
/// `FactSummary` ever gains `confirmed_at`, restore it as the second key.
pub fn rank_highlights(
    summaries: &[FactSummary],
    degree: &HashMap<FactId, usize>,
    limit: usize,
) -> Vec<FactSummary> {
    let mut confirmed: Vec<&FactSummary> = summaries.iter().filter(|s| s.confirmed).collect();

    confirmed.sort_by(|a, b| {
        degree
            .get(&b.id)
            .unwrap_or(&0)
            .cmp(degree.get(&a.id).unwrap_or(&0))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
    confirmed.dedup_by(|a, b| a.id == b.id);
    confirmed.truncate(limit);

    confirmed.into_iter().cloned().collect()
}

/// Convenience wrapper for callers that already hold full facts (tests, and any
/// path where the links are in hand). Applies the same ordering as
/// [`rank_highlights`].
pub fn select_highlights(facts: &[Fact], limit: usize) -> Vec<FactSummary> {
    let degree: HashMap<FactId, usize> = facts.iter().map(|f| (f.id, f.links.len())).collect();
    let summaries: Vec<FactSummary> = facts
        .iter()
        .map(|f| FactSummary {
            id: f.id,
            title: f.title.clone(),
            scope: f.metadata.scope,
            confirmed: f.metadata.confirmed,
        })
        .collect();
    rank_highlights(&summaries, &degree, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::testgen::{fact, linked_fact};

    #[test]
    fn only_confirmed_facts_are_highlighted() {
        let facts = vec![fact("확정", "b", true), fact("미확정", "b", false)];
        let out = select_highlights(&facts, 9);
        assert_eq!(out.len(), 1);
        assert!(out[0].confirmed);
    }

    #[test]
    fn more_connected_facts_come_first() {
        let a = linked_fact("연결 많음", 3);
        let b = linked_fact("연결 적음", 1);
        // Feed them in the "wrong" order to prove ordering is by degree.
        let out = select_highlights(&[b, a], 9);
        assert_eq!(out[0].title, "연결 많음");
    }

    #[test]
    fn limit_is_respected() {
        let facts: Vec<_> = (0..30)
            .map(|i| fact(&format!("사실 {i:02}"), "본문", true))
            .collect();
        assert_eq!(select_highlights(&facts, 9).len(), 9);
        assert_eq!(select_highlights(&facts, 0).len(), 0);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(select_highlights(&[], DEFAULT_HIGHLIGHTS).is_empty());
    }
}
