//! Sharing contract (Rust surface of the canonical MCP contract).
//!
//! The **canonical, team-ratified contract is `docs/mcp-contract.md`.** This
//! module is its Rust encoding — the access model, token/scope rule, and the
//! read-only tool trait an MCP server exposes. The real MCP transport, token
//! minting/validation, and transmission-boundary enforcement live in the owner's
//! MCP vertical, built on top of this.
//!
//! Invariant (do not weaken, `mcp-contract.md` §6): identity and grants come
//! from the [`Token`] only; a consumer sees exactly
//! `granted_categories ∩ {visibility == Shared}`. Tool arguments never widen it.

use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::core::types::{Category, Fact, FactId, FactSummary, Visibility};

mod knowledge;
pub use knowledge::KnowledgeSharing;

/// Contract error codes (`mcp-contract.md` §5). Which layer produces each:
/// `NotFound` = tool domain (this trait) for absent *or* out-of-scope targets —
/// existence is never leaked. `Unauthorized`/`Locked`/`Unavailable`/`InvalidInput`
/// are produced by the transport/auth/store layers of the MCP vertical, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessError {
    /// Absent, or outside the token's scope (indistinguishable on purpose).
    NotFound,
    /// No token, malformed, or revoked.
    Unauthorized,
    /// Vault is locked.
    Locked,
    /// App not running / store unreachable.
    Unavailable,
    /// Bad argument (e.g. category normalization failed).
    InvalidInput,
}

/// A consumer's identity and the categories it may read.
///
/// Grants are fixed at mint time and carried by the token; never accepted as a
/// request argument.
#[derive(Clone, Debug)]
pub struct Token {
    pub id: String,
    /// Categories this token may read (only meaningful for non-owner tokens).
    pub granted: BTreeSet<Category>,
    /// The owner token reads everything, including `Private`, all categories.
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
    pub fn consumer(id: impl Into<String>, granted: impl IntoIterator<Item = Category>) -> Self {
        Self {
            id: id.into(),
            granted: granted.into_iter().collect(),
            owner: false,
        }
    }

    /// The single access rule (`mcp-contract.md` §1). Readable iff owner, or the
    /// fact is `Shared` **and** its category is in the granted set. Category and
    /// visibility are an AND — checking only one is a bug. A `Shared` fact with no
    /// category is unreachable by any consumer.
    pub fn can_access(&self, visibility: Visibility, category: Option<&Category>) -> bool {
        if self.owner {
            return true;
        }
        visibility == Visibility::Shared && category.is_some_and(|c| self.granted.contains(c))
    }

    /// Whether this token may read a given category at all (owner, or granted).
    fn grants(&self, category: &Category) -> bool {
        self.owner || self.granted.contains(category)
    }
}

/// The read-only knowledge surface an MCP server exposes to a token-bearing
/// caller (`mcp-contract.md` §3). Enforcement is a single server-side point (the
/// implementation), never the caller. No tool takes a `category` argument that
/// could widen scope — see §3.2.
#[async_trait]
pub trait SharingApi: Send + Sync {
    /// Categories this token can reach (§3.1).
    async fn list_categories(&self, token: &Token) -> Result<Vec<String>, AccessError>;

    /// Search within the token's scope only. No `category` arg by design (§3.2).
    async fn search_knowledge(
        &self,
        token: &Token,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactSummary>, AccessError>;

    /// Fetch one page, or `NotFound` if absent or out of scope (§3.3).
    async fn get_page(&self, token: &Token, id: FactId) -> Result<Fact, AccessError>;

    /// Owner guidance for one category (§3.4). `NotFound` if that category is not
    /// granted — existence of the category is never revealed.
    async fn get_guide(&self, token: &Token, category: &Category) -> Result<String, AccessError>;
}

/// In-memory `SharingApi` so the work-packages can build/test in parallel before
/// the real store + MCP server land.
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
        self.facts
            .iter()
            .filter(move |f| token.can_access(f.metadata.visibility, f.metadata.category.as_ref()))
    }
}

#[async_trait]
impl SharingApi for MockSharing {
    async fn list_categories(&self, token: &Token) -> Result<Vec<String>, AccessError> {
        let cats: BTreeSet<String> = self
            .visible(token)
            .filter_map(|f| f.metadata.category.as_ref().map(|c| c.as_str().to_string()))
            .collect();
        Ok(cats.into_iter().collect())
    }

    async fn search_knowledge(
        &self,
        token: &Token,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactSummary>, AccessError> {
        Ok(self
            .visible(token)
            .filter(|f| f.title.contains(query) || f.body.contains(query))
            .take(limit)
            .map(|f| FactSummary {
                id: f.id,
                title: f.title.clone(),
                scope: f.metadata.scope,
                confirmed: f.metadata.confirmed,
            })
            .collect())
    }

    async fn get_page(&self, token: &Token, id: FactId) -> Result<Fact, AccessError> {
        self.visible(token)
            .find(|f| f.id == id)
            .cloned()
            .ok_or(AccessError::NotFound)
    }

    async fn get_guide(&self, token: &Token, category: &Category) -> Result<String, AccessError> {
        if !token.grants(category) {
            return Err(AccessError::NotFound);
        }
        Ok(self.guide.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactMetadata, Provenance, Scope, SourceKind};
    use chrono::Utc;

    fn cat(s: &str) -> Category {
        Category::parse(s).unwrap()
    }

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
                category: category.map(cat),
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
        assert_eq!(s.search_knowledge(&t, "body", 50).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn consumer_sees_only_granted_shared() {
        let s = store();
        let t = Token::consumer("teammate", [cat("deploy")]);
        // only fact "b" (Shared + deploy). "a" Private, "c" wrong category, "d" no category.
        assert_eq!(s.search_knowledge(&t, "body", 50).await.unwrap().len(), 1);
        assert_eq!(
            s.list_categories(&t).await.unwrap(),
            vec!["deploy".to_string()]
        );
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let s = store();
        let t = Token::owner();
        assert_eq!(s.search_knowledge(&t, "body", 2).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn get_page_hides_out_of_scope_as_not_found() {
        // The private page exists, but must look absent to a consumer token.
        let private = fact("a", Visibility::Private, Some("deploy"));
        let private_id = private.id;
        let shared = fact("b", Visibility::Shared, Some("deploy"));
        let s = MockSharing::new(vec![private, shared], "guide");
        let t = Token::consumer("teammate", [cat("deploy")]);
        assert!(matches!(
            s.get_page(&t, private_id).await,
            Err(AccessError::NotFound)
        ));
    }

    #[tokio::test]
    async fn get_guide_hides_ungranted_category() {
        let s = store();
        let t = Token::consumer("teammate", [cat("deploy")]);
        assert!(s.get_guide(&t, &cat("deploy")).await.is_ok());
        // not granted → NotFound (never reveal the category exists).
        assert!(matches!(
            s.get_guide(&t, &cat("workstyle")).await,
            Err(AccessError::NotFound)
        ));
    }

    #[test]
    fn category_normalizes() {
        assert_eq!(cat(" Deploy ").as_str(), "deploy");
        assert_eq!(cat("payment service").as_str(), "payment-service");
        assert_eq!(cat("--a__b  c--").as_str(), "a-b-c");
        assert_eq!(cat("결제-서비스").as_str(), "결제-서비스");
        assert!(Category::parse("   ").is_err());
        assert!(Category::parse(&"x".repeat(65)).is_err());
    }
}
