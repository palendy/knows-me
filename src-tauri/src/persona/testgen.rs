//! Reusable domain generators and fixtures for U4 tests (PBT-07).
//!
//! Two audiences:
//! - example-based tests use the plain `fact` / `linked_fact` builders
//! - property-based tests use the proptest strategies below
//!
//! No test constructs domain values from raw primitives directly — every
//! generator here respects the business constraints (valid UUIDs, non-empty
//! titles, `confirmed_at` present iff confirmed, edges only between real nodes).

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;
use uuid::Uuid;

use crate::core::types::{
    DraftKind, DraftRequest, Fact, FactId, FactMetadata, GraphDto, GraphEdge, GraphNode,
    Provenance, Scope, SourceKind,
};

// ---------------------------------------------------------------------------
// Plain fixtures (example-based tests)
// ---------------------------------------------------------------------------

pub fn fact(title: &str, body: &str, confirmed: bool) -> Fact {
    fact_with(title, body, confirmed, Scope::Unknown)
}

pub fn fact_with(title: &str, body: &str, confirmed: bool, scope: Scope) -> Fact {
    fact_with_id(FactId::new(), title, body, confirmed, scope)
}

/// Same as [`fact_with`] but with the id supplied — used by property tests so
/// every value in play comes from proptest's seeded RNG (PBT-08).
pub fn fact_with_id(id: FactId, title: &str, body: &str, confirmed: bool, scope: Scope) -> Fact {
    Fact {
        id,
        title: title.to_string(),
        body: body.to_string(),
        links: vec![],
        metadata: FactMetadata {
            provenance: Provenance {
                source: SourceKind::Session,
                collected_at: Utc::now(),
            },
            confirmed,
            scope,
            confirmed_at: confirmed.then(Utc::now),
            visibility: Default::default(),
            category: None,
        },
    }
}

/// A confirmed fact with `degree` outgoing links (targets are arbitrary ids).
pub fn linked_fact(title: &str, degree: usize) -> Fact {
    let mut f = fact(title, "본문", true);
    f.links = (0..degree).map(|_| FactId::new()).collect();
    f
}

// ---------------------------------------------------------------------------
// proptest strategies (PBT-07: domain generators, not raw primitives)
// ---------------------------------------------------------------------------

/// Ids drawn from proptest's own RNG rather than `Uuid::new_v4()`.
///
/// This is what makes a failure reproducible: `Uuid::new_v4()` reads a global
/// RNG that `PROPTEST_SEED` does not control, so a property that fails on a
/// particular id would never replay — and the shrunk case recorded in
/// `proptest-regressions/` would be meaningless (PBT-08).
pub fn arb_fact_id() -> impl Strategy<Value = FactId> {
    any::<u128>().prop_map(|n| FactId(Uuid::from_u128(n)))
}

/// Titles/bodies that look like real content: non-empty, mixed scripts,
/// including boundary cases (single char, whitespace-heavy, unicode).
pub fn arb_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[가-힣a-zA-Z0-9 ]{1,40}",
        1 => Just("a".to_string()),
        1 => Just("배포 절차".to_string()),
        1 => Just("  공백  많은  본문  ".to_string()),
        1 => "[\\p{Hangul}]{1,20}",
    ]
}

fn arb_scope() -> impl Strategy<Value = Scope> {
    prop_oneof![
        Just(Scope::Company),
        Just(Scope::Personal),
        Just(Scope::Unknown),
    ]
}

fn arb_confirmed_at(confirmed: bool) -> impl Strategy<Value = Option<DateTime<Utc>>> {
    // A bounded, realistic instant range keeps the ordering tiebreaker meaningful.
    (0i64..1_000i64).prop_map(move |d| {
        confirmed.then(|| Utc.timestamp_opt(1_700_000_000 + d * 3_600, 0).unwrap())
    })
}

/// A single fact. `confirmed_at` is present exactly when `confirmed` is true.
pub fn arb_fact() -> impl Strategy<Value = Fact> {
    (
        arb_fact_id(),
        arb_text(),
        arb_text(),
        any::<bool>(),
        arb_scope(),
        prop::collection::vec(arb_fact_id(), 0..5),
    )
        .prop_flat_map(|(id, title, body, confirmed, scope, links)| {
            arb_confirmed_at(confirmed).prop_map(move |confirmed_at| Fact {
                id,
                title: title.clone(),
                body: body.clone(),
                links: links.clone(),
                metadata: FactMetadata {
                    provenance: Provenance {
                        source: SourceKind::Session,
                        collected_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    },
                    confirmed,
                    scope,
                    confirmed_at,
                    visibility: Default::default(),
                    category: None,
                },
            })
        })
}

/// A fact collection, including the empty case.
pub fn arb_facts() -> impl Strategy<Value = Vec<Fact>> {
    prop::collection::vec(arb_fact(), 0..25)
}

/// A graph whose edges point at real nodes, plus deliberately injected
/// dangling edges so normalization has something to remove.
pub fn arb_graph() -> impl Strategy<Value = GraphDto> {
    prop::collection::vec((arb_fact_id(), arb_text()), 0..15).prop_flat_map(|raw| {
        // Ids can repeat by construction; a node *set* must not.
        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (id, label) in raw {
            if seen.insert(id.0) {
                nodes.push(GraphNode { id, label });
            }
        }
        let ids: Vec<FactId> = nodes.iter().map(|n| n.id).collect();
        let n = ids.len();
        let real_edges = if n == 0 {
            prop::collection::vec((0usize..1, 0usize..1), 0..1).boxed()
        } else {
            prop::collection::vec((0..n, 0..n), 0..20).boxed()
        };
        (
            Just(nodes),
            real_edges,
            prop::collection::vec(arb_fact_id(), 0..3),
        )
            .prop_map(move |(nodes, pairs, dangling)| {
                let mut edges: Vec<GraphEdge> = if ids.is_empty() {
                    vec![]
                } else {
                    pairs
                        .into_iter()
                        .map(|(a, b)| GraphEdge {
                            from: ids[a],
                            to: ids[b],
                        })
                        .collect()
                };
                for ghost in dangling {
                    if let Some(&from) = ids.first() {
                        edges.push(GraphEdge { from, to: ghost });
                    }
                }
                GraphDto { nodes, edges }
            })
    })
}

/// A draft request across all three kinds.
pub fn arb_draft_request() -> impl Strategy<Value = DraftRequest> {
    (
        prop_oneof![
            Just(DraftKind::Email),
            Just(DraftKind::Message),
            Just(DraftKind::Post)
        ],
        arb_text(),
    )
        .prop_map(|(kind, prompt)| DraftRequest { kind, prompt })
}

/// Strings that contain identifiers a masker is expected to strip.
pub fn arb_sensitive_text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("연락처는 hong@example.com 입니다".to_string()),
        Just("전화 010-1234-5678 로 주세요".to_string()),
        Just("https://internal.corp/secret 문서 참고".to_string()),
        arb_text(),
    ]
}
