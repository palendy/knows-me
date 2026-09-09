//! Sharing contract (frozen interface for the strengthened concept).
//!
//! This module defines *only the surface* that the four work-packages code
//! against — the access model, the token/scope rule, and the tool trait an MCP
//! server exposes. The real MCP transport, token minting/validation over HTTP,
//! and transmission-boundary enforcement (task **T0**, owner-provided) live
//! elsewhere; this file is the neutral contract everyone builds on.
//!
//! Invariant (do not weaken): identity and grants come from the [`Token`] only.
//! Call arguments (e.g. `category`) may *narrow* within the granted scope — they
//! can never widen it. See `docs/contract-and-tasks.md`.

use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::core::types::{Fact, FactId, FactSummary, Visibility};

/// Why a sharing call was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessError {
    /// No token, or the token is invalid.
    Unauthorized,
    /// Authenticated, but the target is outside the token's granted scope.
    /// Also returned for non-existent targets, so existence is never leaked.
    Forbidden,
}

/// A consumer's identity and the categories it may read.
///
/// Grants are fixed at mint time and carried by the token; they are never
/// accepted as request arguments.
#[derive(Clone, Debug)]
pub struct Token {
    pub id: String,
    /// Categories this token may read (only meaningful for non-owner tokens).
    pub granted: BTreeSet<String>,
    /// The owner token reads everything, including `Private` facts.
    pub owner: bool,
}

impl Token {
    /// The owner's own token — full scope, including `Private`.
    pub fn owner() -> Self {
        Self {
            id: "owner".into(),
            granted: BTreeSet::new(),
            owner: true,
        }
    }

    /// A consumer token scoped to a set of granted categories.
    pub fn consumer(id: impl Into<String>, granted: impl IntoIterator<Item = String>) -> Self {
        Self {
            id: id.into(),
            granted: granted.into_iter().collect(),
            owner: false,
        }
    }

    /// The single access rule. A fact is readable iff:
    /// - this is the owner token, or
    /// - the fact is `Shared` **and** its category is in the granted set.
    ///
    /// A `Shared` fact with no category is not reachable by any consumer.
    pub fn can_access(&self, visibility: Visibility, category: Option<&str>) -> bool {
        if self.owner {
            return true;
        }
        visibility == Visibility::Shared && category.is_some_and(|c| self.granted.contains(c))
    }
}

/// The knowledge surface an MCP server exposes to a token-bearing caller.
///
/// Every method takes the caller's resolved [`Token`]; enforcement is a single
/// server-side point (this trait's implementations), never the caller.
#[async_trait]
pub trait SharingApi: Send + Sync {
    /// Categories this token can reach.
    async fn list_categories(&self, token: &Token) -> Result<Vec<String>, AccessError>;

    /// Search within the token's scope. `category` narrows further (never widens).
    async fn search_knowledge(
        &self,
        token: &Token,
        query: &str,
        category: Option<&str>,
    ) -> Result<Vec<FactSummary>, AccessError>;

    /// Fetch one fact, or `Forbidden` if out of scope (or absent).
    async fn get_fact(&self, token: &Token, id: FactId) -> Result<Fact, AccessError>;

    /// The owner's guidance text (stands in for personal onboarding).
    async fn get_guide(&self, token: &Token, category: Option<&str>)
        -> Result<String, AccessError>;
}

/// In-memory `SharingApi` so the four work-packages can build/test in parallel
/// before the real store + MCP server (T0) land.
pub struct MockSharing {
    facts: Vec<Fact>,
    guide: String,
}

impl MockSharing {
    pub fn new(facts: Vec<Fact>, guide: impl Into<String>) -> Self {
        Self {
            facts,
            guide: guide.into(),
        }
    }

    fn visible<'a>(&'a self, token: &'a Token) -> impl Iterator<Item = &'a Fact> {
        self.facts.iter().filter(move |f| {
            token.can_access(f.metadata.visibility, f.metadata.category.as_deref())
        })
    }
}

#[async_trait]
impl SharingApi for MockSharing {
    async fn list_categories(&self, token: &Token) -> Result<Vec<String>, AccessError> {
        let cats: BTreeSet<String> = self
            .visible(token)
            .filter_map(|f| f.metadata.category.clone())
            .collect();
        Ok(cats.into_iter().collect())
    }

    async fn search_knowledge(
        &self,
        token: &Token,
        query: &str,
        category: Option<&str>,
    ) -> Result<Vec<FactSummary>, AccessError> {
        Ok(self
            .visible(token)
            .filter(|f| category.is_none_or(|c| f.metadata.category.as_deref() == Some(c)))
            .filter(|f| f.title.contains(query) || f.body.contains(query))
            .map(|f| FactSummary {
                id: f.id,
                title: f.title.clone(),
                scope: f.metadata.scope,
                confirmed: f.metadata.confirmed,
            })
            .collect())
    }

    async fn get_fact(&self, token: &Token, id: FactId) -> Result<Fact, AccessError> {
        self.visible(token)
            .find(|f| f.id == id)
            .cloned()
            .ok_or(AccessError::Forbidden)
    }

    async fn get_guide(
        &self,
        _token: &Token,
        _category: Option<&str>,
    ) -> Result<String, AccessError> {
        Ok(self.guide.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactMetadata, Provenance, Scope, SourceKind};
    use chrono::Utc;

    fn fact(id: &str, visibility: Visibility, category: Option<&str>) -> Fact {
        Fact {
            id: FactId::new(),
            title: format!("title {id}"),
            body: format!("body {id}"),
            links: vec![],
            metadata: FactMetadata {
                provenance: Provenance {
                    source: SourceKind::Session,
                    collected_at: Utc::now(),
                },
                confirmed: true,
                scope: Scope::Unknown,
                confirmed_at: Some(Utc::now()),
                visibility,
                category: category.map(str::to_string),
            },
        }
    }

    fn store() -> MockSharing {
        MockSharing::new(
            vec![
                fact("a", Visibility::Private, Some("deploy")),
                fact("b", Visibility::Shared, Some("deploy")),
                fact("c", Visibility::Shared, Some("workstyle")),
                fact("d", Visibility::Shared, None), // shared but uncategorized → unreachable
            ],
            "owner guide",
        )
    }

    #[tokio::test]
    async fn owner_sees_everything() {
        let s = store();
        let t = Token::owner();
        assert_eq!(s.search_knowledge(&t, "body", None).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn consumer_sees_only_granted_shared() {
        let s = store();
        let t = Token::consumer("teammate", ["deploy".to_string()]);
        // only fact "b" (Shared + deploy). "a" is Private, "c" wrong category, "d" no category.
        let hits = s.search_knowledge(&t, "body", None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            s.list_categories(&t).await.unwrap(),
            vec!["deploy".to_string()]
        );
    }

    #[tokio::test]
    async fn category_arg_only_narrows() {
        let s = store();
        let t = Token::consumer("teammate", ["deploy".to_string(), "workstyle".to_string()]);
        assert_eq!(s.search_knowledge(&t, "body", None).await.unwrap().len(), 2);
        assert_eq!(
            s.search_knowledge(&t, "body", Some("workstyle"))
                .await
                .unwrap()
                .len(),
            1
        );
        // asking for a category not granted yields nothing (cannot widen).
        assert_eq!(
            s.search_knowledge(&t, "body", Some("secret"))
                .await
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn get_fact_hides_out_of_scope_as_forbidden() {
        // The private fact exists in the store, but must look absent to a
        // consumer token — same `Forbidden` as a non-existent id (no leak).
        let private = fact("a", Visibility::Private, Some("deploy"));
        let private_id = private.id;
        let shared = fact("b", Visibility::Shared, Some("deploy"));
        let s = MockSharing::new(vec![private, shared], "guide");
        let t = Token::consumer("teammate", ["deploy".to_string()]);
        assert!(matches!(
            s.get_fact(&t, private_id).await,
            Err(AccessError::Forbidden)
        ));
    }
}
