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

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::core::types::{
    Fact, FactFilter, FactId, FactSummary, GraphDto, GraphEdge, GraphFilter, GraphNode, Scope,
};

/// Lightweight per-fact metadata kept in RAM (no body).
#[derive(Clone)]
struct MetaLite {
    title: String,
    scope: Scope,
    confirmed: bool,
    confirmed_at: Option<DateTime<Utc>>,
    links: Vec<FactId>,
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
                confirmed: m.confirmed,
            })
            .collect()
    }

    /// Graph projection: nodes = (scope-filtered) facts, edges = stored links
    /// whose target is also in the node set (KR-5 — dangling links skipped).
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
        GraphDto { nodes, edges }
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
