//! Property-based tests for U4 (PBT-02, PBT-03).
//!
//! Kept in their own module so property tests and example-based tests stay
//! visibly separate (PBT-10). Generators come from
//! [`testgen`](super::testgen) — no raw-primitive strategies (PBT-07).
//!
//! Reproducing a failure: proptest prints the shrunk minimal case and writes it
//! to `src-tauri/proptest-regressions/`, which is committed. A specific run can
//! be replayed with `PROPTEST_SEED=<seed> cargo test` (PBT-08).

#![cfg(test)]

use proptest::prelude::*;
use std::sync::Arc;

use crate::core::traits::{KnowledgeApi, LlmClient, Masker, PersonaApi};
use crate::core::types::{Fact, MaskedText, UnmaskMap};
use crate::mocks::InMemoryKnowledge;
use crate::persona::local_api::{
    ChatRequestBody, DraftRequestBody, ErrorBody, HealthBody, TextResponseBody,
};
use crate::persona::selection::select_highlights;
use crate::persona::testgen::{
    arb_draft_request, arb_fact_id, arb_facts, arb_sensitive_text, arb_text,
};
use crate::persona::{context::select_context, ContextSelection, PersonaService};

/// Deterministic shuffle so "same multiset, different order" can be tested
/// without pulling in a RNG dependency.
fn rotate<T: Clone>(v: &[T], by: usize) -> Vec<T> {
    if v.is_empty() {
        return vec![];
    }
    let by = by % v.len();
    let mut out = v[by..].to_vec();
    out.extend_from_slice(&v[..by]);
    out
}

proptest! {
    // ----------------------------------------------------------------
    // PBT-03 — context assembly invariants (BR-P1, BR-P5, BR-P7, C1-C3)
    // ----------------------------------------------------------------

    /// C1: an unconfirmed fact can never become grounding for an answer.
    #[test]
    fn context_contains_only_confirmed_facts(facts in arb_facts(), q in arb_text()) {
        let population = facts.iter().filter(|f| f.metadata.confirmed).count();
        let ctx = select_context(&facts, &q, &ContextSelection::default(), population);
        let confirmed_ids: Vec<_> = facts
            .iter()
            .filter(|f| f.metadata.confirmed)
            .map(|f| f.id)
            .collect();
        for e in &ctx.entries {
            prop_assert!(confirmed_ids.contains(&e.id));
        }
    }

    /// C2: the prompt size is bounded no matter how large the knowledge base is.
    #[test]
    fn context_never_exceeds_max_facts(
        facts in arb_facts(),
        q in arb_text(),
        max in 0usize..20,
    ) {
        let sel = ContextSelection { max_facts: max, scope: None };
        let ctx = select_context(&facts, &q, &sel, facts.len());
        prop_assert!(ctx.entries.len() <= max);
    }

    /// C3: duplicates in the search result do not duplicate grounding.
    #[test]
    fn context_has_no_duplicate_fact_ids(facts in arb_facts(), q in arb_text()) {
        let ctx = select_context(&facts, &q, &ContextSelection::default(), facts.len());
        let mut ids: Vec<_> = ctx.entries.iter().map(|e| e.id.0).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        prop_assert_eq!(before, ids.len());
    }

    /// BR-P7: the sort key is a total order, so input order cannot leak into
    /// the answer. Same facts in a different order -> identical context.
    #[test]
    fn context_is_invariant_under_input_order(
        facts in arb_facts(),
        q in arb_text(),
        by in 0usize..25,
    ) {
        let sel = ContextSelection::default();
        let a = select_context(&facts, &q, &sel, facts.len());
        let b = select_context(&rotate(&facts, by), &q, &sel, facts.len());
        prop_assert_eq!(a, b);
    }

    /// total_confirmed reports the population the caller measured, not the
    /// bounded candidate slice the selection ran over (E1).
    #[test]
    fn total_confirmed_reports_the_population(
        facts in arb_facts(),
        q in arb_text(),
        extra in 0usize..1000,
    ) {
        let sel = ContextSelection { max_facts: 2, scope: None };
        let confirmed = facts.iter().filter(|f| f.metadata.confirmed).count();
        // The population is at least what is on hand, and usually larger.
        let population = confirmed + extra;
        let ctx = select_context(&facts, &q, &sel, population);
        prop_assert_eq!(ctx.total_confirmed, population);
        prop_assert!(ctx.entries.len() <= confirmed);
    }

    // ----------------------------------------------------------------
    // PBT-03 — highlight selection invariants (BR-V2)
    // ----------------------------------------------------------------

    /// Output is a subset of the confirmed input, within the limit.
    #[test]
    fn highlights_are_a_bounded_confirmed_subset(facts in arb_facts(), limit in 0usize..15) {
        let out = select_highlights(&facts, limit);
        prop_assert!(out.len() <= limit);

        let confirmed_ids: Vec<_> = facts
            .iter()
            .filter(|f| f.metadata.confirmed)
            .map(|f| f.id)
            .collect();
        for s in &out {
            prop_assert!(confirmed_ids.contains(&s.id));
            prop_assert!(s.confirmed);
        }
    }

    /// Same facts, different order -> same highlights (BR-V4 determinism).
    #[test]
    fn highlights_are_invariant_under_input_order(facts in arb_facts(), by in 0usize..25) {
        let a = select_highlights(&facts, 9);
        let b = select_highlights(&rotate(&facts, by), 9);
        prop_assert_eq!(
            a.iter().map(|s| s.id.0).collect::<Vec<_>>(),
            b.iter().map(|s| s.id.0).collect::<Vec<_>>()
        );
    }

    /// Highlights are ordered by descending link degree.
    #[test]
    fn highlights_are_ordered_by_degree(facts in arb_facts()) {
        let out = select_highlights(&facts, 20);
        let degrees: Vec<usize> = out
            .iter()
            .map(|s| {
                facts
                    .iter()
                    .find(|f| f.id == s.id)
                    .map(|f| f.links.len())
                    .unwrap_or(0)
            })
            .collect();
        for w in degrees.windows(2) {
            prop_assert!(w[0] >= w[1]);
        }
    }

    // ----------------------------------------------------------------
    // PBT-02 — wire DTO round-trips
    // ----------------------------------------------------------------

    #[test]
    fn chat_request_json_round_trips(prompt in arb_sensitive_text()) {
        let original = ChatRequestBody {
            prompt,
            history: vec![],
        };
        let decoded: ChatRequestBody =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        prop_assert_eq!(original, decoded);
    }

    #[test]
    fn draft_request_json_round_trips(req in arb_draft_request()) {
        let original = DraftRequestBody { kind: req.kind.into(), prompt: req.prompt };
        let decoded: DraftRequestBody =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        prop_assert_eq!(original, decoded);
    }

    #[test]
    fn text_response_json_round_trips(text in arb_sensitive_text()) {
        let original = TextResponseBody { text };
        let decoded: TextResponseBody =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        prop_assert_eq!(original, decoded);
    }

    #[test]
    fn error_and_health_json_round_trip(msg in arb_text(), persona in any::<bool>()) {
        let e = ErrorBody { error: msg.clone() };
        let decoded: ErrorBody =
            serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        prop_assert_eq!(e, decoded);

        let h = HealthBody { status: msg, persona };
        let decoded: HealthBody =
            serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        prop_assert_eq!(h, decoded);
    }

    /// Facts themselves round-trip through the persistence format U3 uses, so
    /// a fact rendered into context survives serialization unchanged.
    #[test]
    fn fact_json_round_trips(facts in arb_facts()) {
        for f in facts {
            let json = serde_json::to_string(&f).unwrap();
            let back: Fact = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(f.id, back.id);
            prop_assert_eq!(&f.title, &back.title);
            prop_assert_eq!(&f.body, &back.body);
            prop_assert_eq!(f.metadata.confirmed, back.metadata.confirmed);
            prop_assert_eq!(f.metadata.confirmed_at, back.metadata.confirmed_at);
        }
    }
}

// ---------------------------------------------------------------------------
// PBT-03 — the masking invariant (BR-P2, U4-NFR-SEC3)
// ---------------------------------------------------------------------------

/// Records every string handed to the gateway so a property can inspect it.
#[derive(Default)]
struct RecordingLlm {
    seen: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LlmClient for RecordingLlm {
    async fn summarize(&self, _i: &MaskedText) -> crate::core::error::Result<String> {
        Ok(String::new())
    }
    async fn classify(&self, _i: &MaskedText) -> crate::core::error::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn vision_extract(&self, _b: &[u8]) -> crate::core::error::Result<MaskedText> {
        Ok(MaskedText {
            text: String::new(),
        })
    }
    async fn chat(&self, system: &str, input: &MaskedText) -> crate::core::error::Result<String> {
        self.seen.lock().unwrap().push(system.to_string());
        self.seen.lock().unwrap().push(input.text.clone());
        Ok("응답".into())
    }
}

/// Stand-in for U1's real masker: strips anything that looks like an email,
/// a phone number or a URL.
struct PatternMasker;

impl Masker for PatternMasker {
    fn mask(&self, text: &str) -> (MaskedText, UnmaskMap) {
        let mut map = UnmaskMap::new();
        let mut out = Vec::new();
        for tok in text.split_whitespace() {
            let sensitive = tok.contains('@')
                || tok.starts_with("http")
                || (tok.chars().filter(|c| c.is_ascii_digit()).count() >= 8);
            if sensitive {
                let ph = format!("[MASK_{}]", map.entries().len());
                map.insert(ph.clone(), tok.to_string());
                out.push(ph);
            } else {
                out.push(tok.to_string());
            }
        }
        (
            MaskedText {
                text: out.join(" "),
            },
            map,
        )
    }
    fn unmask(&self, masked: &MaskedText, map: &UnmaskMap) -> String {
        let mut s = masked.text.clone();
        for (ph, orig) in map.entries() {
            s = s.replace(ph, orig);
        }
        s
    }
}

proptest! {
    /// No matter what the owner's facts or question contain, the gateway never
    /// sees a raw identifier — masking always runs first (BR-P2).
    #[test]
    fn identifiers_never_reach_the_gateway(
        bodies in prop::collection::vec((arb_fact_id(), arb_sensitive_text()), 1..6),
        question in arb_sensitive_text(),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let sensitive: Vec<String> = bodies
            .iter()
            .map(|(_, body)| body)
            .chain(std::iter::once(&question))
            .flat_map(|s| s.split_whitespace())
            .filter(|t| {
                t.contains('@')
                    || t.starts_with("http")
                    || t.chars().filter(|c| c.is_ascii_digit()).count() >= 8
            })
            .map(|t| t.to_string())
            .collect();

        let llm = Arc::new(RecordingLlm::default());
        let llm_for_svc: Arc<dyn LlmClient> = llm.clone();

        rt.block_on(async {
            let kn = Arc::new(InMemoryKnowledge::default());
            for (i, (id, body)) in bodies.iter().enumerate() {
                let f = crate::persona::testgen::fact_with_id(
                    *id,
                    &format!("사실 {i}"),
                    body,
                    true,
                    crate::core::types::Scope::Unknown,
                );
                kn.upsert(f).await.unwrap();
            }
            let svc = PersonaService::new(kn, Arc::new(PatternMasker), llm_for_svc);
            // Errors are fine here (blank/oversized prompts); the invariant is
            // about what reached the gateway, not about the call succeeding.
            let _ = svc.chat(question.clone(), vec![]).await;
        });

        let seen = llm.seen.lock().unwrap();
        for text in seen.iter() {
            for s in &sensitive {
                prop_assert!(
                    !text.contains(s.as_str()),
                    "raw identifier {s:?} reached the LLM gateway in {text:?}"
                );
            }
        }
    }
}
