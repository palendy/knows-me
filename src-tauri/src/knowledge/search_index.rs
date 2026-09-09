//! `SearchIndex` — in-memory, derived index + metadata cache.
//!
//! Powers `search`, `graph`, and `dashboard` from RAM (NFR-4) without loading
//! fact bodies from disk. It is *derived* state: rebuilt from `FactStore` on
//! unlock and updated incrementally on every upsert (P-1/P-5). Fact bodies stay
//! in the encrypted store and are loaded only by `get`.
//!
//! Tokenization (P-3): Latin/numeric runs → lowercased words; CJK runs (Hangul,
//! Kana, Han) → 2-grams, giving practical Korean substring search without a
//! morphological analyzer.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::core::types::{
    Fact, FactFilter, FactId, FactKind, FactSummary, GraphDto, GraphEdge, GraphFilter, GraphNode,
    Scope, TopicPage, Visibility,
};

/// Assemble topic pages from the indexed facts.
///
/// Ranking is repetition first, then recency: a subject mentioned once is not
/// an interest, and a subject last touched in March is not a current one.
/// Friction counts are carried separately so a caller can ask for what is
/// unresolved rather than what is merely frequent.
pub fn topic_pages(
    facts: &[FactSummary],
    seen_at: &HashMap<FactId, Option<DateTime<Utc>>>,
) -> Vec<TopicPage> {
    // Each item is classified alone, so the model cannot see what it called the
    // same subject last time: `ai-dlc` and `aidlc` arrive as two topics and each
    // looks like a one-off. Grouping on the separator-free form merges them, and
    // the most common surface spelling names the page.
    let mut groups: BTreeMap<String, (BTreeMap<String, usize>, Vec<&FactSummary>)> =
        BTreeMap::new();
    for f in facts {
        for t in &f.topics {
            let key: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
            if key.is_empty() {
                continue;
            }
            let entry = groups.entry(key).or_default();
            *entry.0.entry(t.clone()).or_insert(0) += 1;
            entry.1.push(f);
        }
    }

    let by_topic: BTreeMap<String, Vec<&FactSummary>> = groups
        .into_values()
        .map(|(surfaces, mut members)| {
            let name = surfaces
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(s, _)| s.clone())
                .unwrap_or_default();
            // One fact can reach a group through two spellings; count it once.
            members.sort_by_key(|f| f.id.0);
            members.dedup_by_key(|f| f.id);
            (name, members)
        })
        .collect();

    let mut pages: Vec<TopicPage> = by_topic
        .into_iter()
        .map(|(topic, members)| {
            let times: Vec<DateTime<Utc>> = members
                .iter()
                .filter_map(|f| seen_at.get(&f.id).copied().flatten())
                .collect();
            let mut facts: Vec<FactSummary> = members.iter().map(|f| (*f).clone()).collect();
            facts.sort_by(|a, b| {
                seen_at
                    .get(&b.id)
                    .copied()
                    .flatten()
                    .cmp(&seen_at.get(&a.id).copied().flatten())
                    .then_with(|| a.title.cmp(&b.title))
            });
            TopicPage {
                topic,
                mentions: members.len(),
                concerns: members
                    .iter()
                    .filter(|f| f.kind == FactKind::Concern)
                    .count(),
                first_seen: times.iter().min().copied(),
                last_seen: times.iter().max().copied(),
                facts,
            }
        })
        .collect();

    pages.sort_by(|a, b| {
        b.mentions
            .cmp(&a.mentions)
            .then_with(|| b.last_seen.cmp(&a.last_seen))
            .then_with(|| a.topic.cmp(&b.topic))
    });
    pages
}

/// Lightweight per-fact metadata kept in RAM (no body).
#[derive(Clone)]
struct MetaLite {
    title: String,
    scope: Scope,
    confirmed: bool,
    confirmed_at: Option<DateTime<Utc>>,
    links: Vec<FactId>,
    kind: FactKind,
    topics: Vec<String>,
    visibility: Visibility,
}

/// In-memory inverted index + metadata cache.
#[derive(Default)]
pub struct SearchIndex {
    postings: HashMap<String, HashSet<FactId>>,
    tokens: HashMap<FactId, HashSet<String>>,
    meta: HashMap<FactId, MetaLite>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.postings.clear();
        self.tokens.clear();
        self.meta.clear();
    }

    /// Number of indexed facts.
    pub fn len(&self) -> usize {
        self.meta.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meta.is_empty()
    }

    /// Insert or update a fact in the index (removes stale postings first).
    pub fn upsert(&mut self, fact: &Fact) {
        self.remove(fact.id);
        let toks = tokenize(&format!("{} {}", fact.title, fact.body));
        for t in &toks {
            self.postings.entry(t.clone()).or_default().insert(fact.id);
        }
        self.tokens.insert(fact.id, toks);
        self.meta.insert(
            fact.id,
            MetaLite {
                title: fact.title.clone(),
                scope: fact.metadata.scope,
                confirmed: fact.metadata.confirmed,
                confirmed_at: fact.metadata.confirmed_at,
                links: fact.links.clone(),
                kind: fact.metadata.kind,
                topics: fact.metadata.topics.clone(),
                visibility: fact.metadata.visibility,
            },
        );
    }

    /// Remove a fact from the index (no-op if absent).
    pub fn remove(&mut self, id: FactId) {
        if let Some(old) = self.tokens.remove(&id) {
            for t in old {
                if let Some(set) = self.postings.get_mut(&t) {
                    set.remove(&id);
                    if set.is_empty() {
                        self.postings.remove(&t);
                    }
                }
            }
        }
        self.meta.remove(&id);
    }

    /// Keyword + scope search (KR-6). Empty query returns all (scope-filtered).
    pub fn search(&self, query: &str, filter: &FactFilter) -> Vec<FactSummary> {
        let qtoks = tokenize(query);
        let ids: Vec<FactId> = if query.trim().is_empty() {
            self.meta.keys().copied().collect()
        } else if qtoks.is_empty() {
            // Non-empty query that produced no searchable tokens ⇒ no matches
            // (do NOT fall through to "return everything").
            return Vec::new();
        } else {
            let mut sets: Vec<&HashSet<FactId>> = Vec::with_capacity(qtoks.len());
            for t in &qtoks {
                match self.postings.get(t) {
                    Some(s) => sets.push(s),
                    None => return Vec::new(), // a token nobody has ⇒ AND yields nothing
                }
            }
            sets.sort_by_key(|s| s.len());
            let mut acc: HashSet<FactId> = sets[0].clone();
            for s in &sets[1..] {
                acc.retain(|id| s.contains(id));
            }
            acc.into_iter().collect()
        };
        ids.into_iter()
            .filter_map(|id| self.meta.get(&id).map(|m| (id, m)))
            .filter(|(_, m)| filter.scope.is_none_or(|s| s == m.scope))
            .map(|(id, m)| FactSummary {
                id,
                title: m.title.clone(),
                scope: m.scope,
                kind: m.kind,
                topics: m.topics.clone(),
                visibility: m.visibility,
                confirmed: m.confirmed,
            })
            .collect()
    }

    /// Graph projection: nodes = (scope-filtered) facts, edges = stored links
    /// plus topic-derived links, both restricted to the node set (KR-5 —
    /// dangling links skipped).
    ///
    /// Facts are stored with no explicit `links` today (nothing populates them),
    /// so a links-only projection is always edgeless — the graph reads as
    /// scattered dots. Topics are the signal that separate observations are
    /// about the same thing, so facts that share a topic are connected here.
    /// To avoid a topic that every fact carries turning into a complete graph
    /// (O(n^2) edges), each topic group is connected as a *chain* of its facts
    /// in id order: n-1 edges per topic, and every member stays reachable.
    /// Duplicate/undirected edges are collapsed by the frontend's
    /// `normalizeGraph`, so a pair sharing several topics is fine here.
    pub fn graph(&self, filter: &GraphFilter) -> GraphDto {
        let node_ids: HashSet<FactId> = self
            .meta
            .iter()
            .filter(|(_, m)| filter.scope.is_none_or(|s| s == m.scope))
            .map(|(id, _)| *id)
            .collect();
        let nodes = node_ids
            .iter()
            .map(|id| GraphNode {
                id: *id,
                label: self.meta[id].title.clone(),
            })
            .collect();

        let mut edges = Vec::new();

        // 1) Explicit stored links (once these are ever populated).
        for id in &node_ids {
            for link in &self.meta[id].links {
                if node_ids.contains(link) {
                    edges.push(GraphEdge {
                        from: *id,
                        to: *link,
                    });
                }
            }
        }

        // 2) Topic-derived links: chain the facts within each shared topic.
        //    Sorted by id so the projection is deterministic across calls.
        let mut by_topic: BTreeMap<&str, Vec<FactId>> = BTreeMap::new();
        for id in &node_ids {
            for topic in &self.meta[id].topics {
                by_topic.entry(topic.as_str()).or_default().push(*id);
            }
        }
        for members in by_topic.values_mut() {
            // FactId isn't Ord; sort by the uuid bytes for a stable order.
            members.sort_unstable_by_key(|id| *id.0.as_bytes());
            for pair in members.windows(2) {
                edges.push(GraphEdge {
                    from: pair[0],
                    to: pair[1],
                });
            }
        }

        GraphDto { nodes, edges }
    }

    /// Every fact's confirmation time, for aggregations that rank by recency.
    pub fn confirmed_at_map(&self) -> HashMap<FactId, Option<DateTime<Utc>>> {
        self.meta
            .iter()
            .map(|(id, m)| (*id, m.confirmed_at))
            .collect()
    }

    /// The `n` most recently confirmed facts (for the dashboard).
    pub fn recent(&self, n: usize) -> Vec<FactSummary> {
        let mut v: Vec<(&FactId, &MetaLite)> = self.meta.iter().collect();
        v.sort_by_key(|(_, m)| std::cmp::Reverse(m.confirmed_at));
        v.into_iter()
            .take(n)
            .map(|(id, m)| FactSummary {
                id: *id,
                title: m.title.clone(),
                scope: m.scope,
                kind: m.kind,
                topics: m.topics.clone(),
                visibility: m.visibility,
                confirmed: m.confirmed,
            })
            .collect()
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // Hiragana + Katakana
        0x3400..=0x4DBF |   // CJK Ext A
        0x4E00..=0x9FFF |   // CJK Unified
        0x1100..=0x11FF |   // Hangul Jamo
        0xAC00..=0xD7A3     // Hangul syllables
    )
}

fn flush_latin(buf: &mut String, out: &mut HashSet<String>) {
    if !buf.is_empty() {
        out.insert(buf.to_lowercase());
        buf.clear();
    }
}

fn flush_cjk(buf: &mut Vec<char>, out: &mut HashSet<String>) {
    match buf.len() {
        0 => {}
        1 => {
            out.insert(buf[0].to_string());
        }
        _ => {
            for w in buf.windows(2) {
                out.insert(w.iter().collect());
            }
        }
    }
    buf.clear();
}

/// Tokenize text into a set of index terms (see module docs, P-3).
pub fn tokenize(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for c in text.chars() {
        if is_cjk(c) {
            flush_latin(&mut latin, &mut out);
            cjk.push(c);
        } else if c.is_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            latin.push(c);
        } else {
            flush_latin(&mut latin, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_latin(&mut latin, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

#[cfg(test)]
mod topic_tests {
    use super::*;
    use crate::core::types::{FactKind, Scope};
    use chrono::TimeZone;

    fn summary(n: u8, kind: FactKind, topics: &[&str]) -> FactSummary {
        FactSummary {
            id: FactId(uuid::Uuid::from_u128(n as u128)),
            title: format!("사실 {n}"),
            scope: Scope::Company,
            kind,
            topics: topics.iter().map(|t| t.to_string()).collect(),
            confirmed: true,
            visibility: Default::default(),
        }
    }

    fn at(facts: &[FactSummary]) -> HashMap<FactId, Option<DateTime<Utc>>> {
        facts
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    f.id,
                    Some(
                        Utc.timestamp_opt(1_700_000_000 + i as i64 * 86_400, 0)
                            .unwrap(),
                    ),
                )
            })
            .collect()
    }

    #[test]
    fn spelling_variants_of_a_subject_become_one_page() {
        // Each note is classified alone, so the same subject arrives spelled
        // differently; left apart, every one looks like a one-off.
        let facts = vec![
            summary(1, FactKind::Practice, &["ai-dlc"]),
            summary(2, FactKind::Practice, &["aidlc"]),
            summary(3, FactKind::Project, &["ai-dlc"]),
        ];
        let pages = topic_pages(&facts, &at(&facts));

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].mentions, 3);
        assert_eq!(
            pages[0].topic, "ai-dlc",
            "the common spelling names the page"
        );
    }

    #[test]
    fn friction_is_counted_separately_from_mentions() {
        let facts = vec![
            summary(1, FactKind::Concern, &["deployment"]),
            summary(2, FactKind::Practice, &["deployment"]),
        ];
        let pages = topic_pages(&facts, &at(&facts));

        assert_eq!(pages[0].mentions, 2);
        assert_eq!(pages[0].concerns, 1);
    }

    #[test]
    fn pages_are_ranked_by_repetition_then_recency() {
        let facts = vec![
            summary(1, FactKind::Note, &["rare"]),
            summary(2, FactKind::Note, &["common"]),
            summary(3, FactKind::Note, &["common"]),
        ];
        let pages = topic_pages(&facts, &at(&facts));

        assert_eq!(
            pages[0].topic, "common",
            "a subject mentioned once is not an interest"
        );
    }

    #[test]
    fn one_fact_reaching_a_group_twice_is_counted_once() {
        let facts = vec![summary(1, FactKind::Note, &["ai-dlc", "aidlc"])];
        let pages = topic_pages(&facts, &at(&facts));

        assert_eq!(pages[0].mentions, 1);
    }
}
