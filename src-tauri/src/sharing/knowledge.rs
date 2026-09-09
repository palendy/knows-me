//! `KnowledgeSharing` — the real [`SharingApi`], backed by [`KnowledgeService`].
//!
//! The store-backed materialization of [`MockSharing`](super::MockSharing): the
//! same access rule ([`Token::can_access`], the single enforcement point per
//! `docs/mcp-contract.md` §6), but sourcing facts from the owner's real U3
//! knowledge store instead of an in-memory `Vec`. It carries the **ⓐ owner
//! self-reference path** end-to-end with no token ([`Token::owner`]); the MCP
//! transport, token minting, and the `<knows-me:content>` envelope layer on top
//! of this in later steps of the vertical.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::core::error::AppError;
use crate::core::traits::KnowledgeApi;
use crate::core::types::{Category, Fact, FactFilter, FactId, FactSummary};
use crate::knowledge::KnowledgeService;
use crate::sharing::{AccessError, SharingApi, Token};

/// [`SharingApi`] over the real [`KnowledgeService`]. Depends on the concrete
/// service (not just [`KnowledgeApi`](crate::core::traits::KnowledgeApi)) because
/// it needs `all_facts` — the owner-scope enumeration the tool trait folds into
/// its own scope filter.
pub struct KnowledgeSharing {
    knowledge: Arc<KnowledgeService>,
}

impl KnowledgeSharing {
    pub fn new(knowledge: Arc<KnowledgeService>) -> Self {
        Self { knowledge }
    }
}

/// Map a U3 [`AppError`] onto the contract's [`AccessError`] (`mcp-contract.md`
/// §5). `Unauthorized` is never produced here — it belongs to the transport/auth
/// layer, not the tool domain.
fn to_access_error(e: AppError) -> AccessError {
    match e {
        AppError::Locked => AccessError::Locked,
        AppError::NotFound(_) => AccessError::NotFound,
        AppError::InvalidInput(_) => AccessError::InvalidInput,
        // Store/crypto/io/serde/external failures mean the same thing to a caller:
        // the knowledge store cannot be reached or read right now.
        AppError::Crypto(_) | AppError::Io(_) | AppError::External(_) | AppError::Serde(_) => {
            AccessError::Unavailable
        }
    }
}

/// The facts a token may read — the §6 access rule applied in one place, over an
/// owner-scope slice. Every enumeration of the full store (`all_facts`) funnels
/// through here so the visibility/category predicate has a single definition
/// (mirrors [`MockSharing::visible`](super::MockSharing)).
fn accessible<'a>(token: &'a Token, facts: &'a [Fact]) -> impl Iterator<Item = &'a Fact> {
    facts
        .iter()
        .filter(move |f| token.can_access(f.metadata.visibility, f.metadata.category.as_ref()))
}

#[async_trait]
impl SharingApi for KnowledgeSharing {
    async fn list_categories(&self, token: &Token) -> Result<Vec<String>, AccessError> {
        let facts = self.knowledge.all_facts().await.map_err(to_access_error)?;
        // A category is reachable iff some fact the token can access carries it;
        // `Private`/ungranted facts never leak their category. BTreeSet sorts+dedups.
        let cats: BTreeSet<String> = accessible(token, &facts)
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
        // Reuse U3's index for match quality (tokenized, Korean-aware). No
        // category/scope narrowing here — the tool takes no such argument (§3.2);
        // the token is the only thing that narrows scope.
        let mut hits = self
            .knowledge
            .search(query.to_string(), FactFilter::default())
            .await
            .map_err(to_access_error)?;
        // The index yields hits in HashSet order (non-deterministic), so when
        // matches exceed `limit` the truncated subset would vary run to run. Impose
        // a stable order (title, then id) before truncating — deterministic and
        // human-legible. Relevance ranking is future work (the index scores no
        // match), and would live here once it exists.
        hits.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.0.cmp(&b.id.0)));
        // Filter to what the token may see, THEN take `limit`, so a consumer never
        // gets fewer results just because out-of-scope hits sorted first. The index
        // summary lacks visibility/category, so reload each hit to decide access.
        let mut out = Vec::new();
        for h in hits {
            if out.len() >= limit {
                break;
            }
            match self.knowledge.get(h.id).await {
                Ok(f) if token.can_access(f.metadata.visibility, f.metadata.category.as_ref()) => {
                    out.push(h)
                }
                Ok(_) => {}                      // indexed, but out of this token's scope
                Err(AppError::NotFound(_)) => {} // raced deletion between index and store
                Err(e) => return Err(to_access_error(e)),
            }
        }
        Ok(out)
    }

    async fn get_page(&self, token: &Token, id: FactId) -> Result<Fact, AccessError> {
        match self.knowledge.get(id).await {
            Ok(f) if token.can_access(f.metadata.visibility, f.metadata.category.as_ref()) => Ok(f),
            // Out of scope must be indistinguishable from absent (§3.3 / §5.1).
            Ok(_) | Err(AppError::NotFound(_)) => Err(AccessError::NotFound),
            Err(e) => Err(to_access_error(e)),
        }
    }

    async fn get_guide(&self, token: &Token, category: &Category) -> Result<String, AccessError> {
        // Existence of an ungranted category is never revealed (§3.4).
        if !token.grants(category) {
            return Err(AccessError::NotFound);
        }
        let facts = self.knowledge.all_facts().await.map_err(to_access_error)?;
        let mut pages: Vec<&Fact> = accessible(token, &facts)
            .filter(|f| f.metadata.category.as_ref() == Some(category))
            .collect();
        // Granted, but nothing in it is visible ⇒ indistinguishable from absent
        // (§5.1), and consistent with `list_categories` (which also omits a
        // category with no visible page). Never return an empty `Ok("")` here.
        if pages.is_empty() {
            return Err(AccessError::NotFound);
        }
        // Stable section order (title, then id): `all_facts` order follows
        // `store.list` (HashMap/read_dir), so without this the guide text reorders
        // run to run and across machines.
        pages.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.0.cmp(&b.id.0)));
        // Interim guide: the visible pages of this category composed into one text.
        // The authored per-category summary (mcp-contract §10.1) is an open U3·U4
        // item; until it lands the guide is derived from the pages themselves. The
        // `<knows-me:content>` envelope (§4.1) is applied later, at serving time.
        let mut guide = String::new();
        for f in pages {
            if !guide.is_empty() {
                guide.push_str("\n\n");
            }
            guide.push_str("## ");
            guide.push_str(&f.title);
            guide.push('\n');
            guide.push_str(&f.body);
        }
        Ok(guide)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactMetadata, Provenance, Scope, SourceKind, Visibility};
    use crate::mocks::InMemoryStore;
    use chrono::Utc;

    fn cat(s: &str) -> Category {
        Category::parse(s).unwrap()
    }

    fn fact(title: &str, visibility: Visibility, category: Option<&str>) -> Fact {
        Fact {
            id: FactId::new(),
            title: title.into(),
            body: format!("body of {title}"),
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
                topics: vec![],
                kind: Default::default(),
            },
        }
    }

    /// A `KnowledgeSharing` over a real `KnowledgeService`/`InMemoryStore`, seeded
    /// with the same fixture the `MockSharing` tests use. Returns the fact ids in
    /// insertion order.
    async fn seeded() -> (KnowledgeSharing, Vec<FactId>) {
        let svc = Arc::new(KnowledgeService::new(Arc::new(InMemoryStore::default())));
        let facts = vec![
            fact("a", Visibility::Private, Some("deploy")),
            fact("b", Visibility::Shared, Some("deploy")),
            fact("c", Visibility::Shared, Some("workstyle")),
            fact("d", Visibility::Shared, None), // shared but uncategorized → unreachable
        ];
        let mut ids = Vec::new();
        for f in facts {
            ids.push(f.id);
            svc.upsert(f).await.unwrap();
        }
        (KnowledgeSharing::new(svc), ids)
    }

    #[tokio::test]
    async fn owner_sees_everything() {
        let (s, _) = seeded().await;
        let t = Token::owner();
        assert_eq!(s.search_knowledge(&t, "body", 50).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn consumer_sees_only_granted_shared() {
        let (s, _) = seeded().await;
        let t = Token::consumer("teammate", [cat("deploy")]);
        // only "b" (Shared + deploy). "a" Private, "c" wrong category, "d" no category.
        assert_eq!(s.search_knowledge(&t, "body", 50).await.unwrap().len(), 1);
        assert_eq!(
            s.list_categories(&t).await.unwrap(),
            vec!["deploy".to_string()]
        );
    }

    #[tokio::test]
    async fn owner_lists_all_categories() {
        let (s, _) = seeded().await;
        let t = Token::owner();
        // deploy + workstyle; "d" has no category so contributes none.
        assert_eq!(
            s.list_categories(&t).await.unwrap(),
            vec!["deploy".to_string(), "workstyle".to_string()]
        );
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let (s, _) = seeded().await;
        let t = Token::owner();
        // All four match "body"; the limit-2 truncation is a *deterministic*
        // prefix in stable (title) order — not an arbitrary HashSet-order subset.
        let titles: Vec<String> = s
            .search_knowledge(&t, "body", 2)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.title)
            .collect();
        assert_eq!(titles, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn get_page_hides_out_of_scope_as_not_found() {
        let (s, ids) = seeded().await;
        let private_a = ids[0]; // fact "a": Private + deploy
        let t = Token::consumer("teammate", [cat("deploy")]);
        assert!(matches!(
            s.get_page(&t, private_a).await,
            Err(AccessError::NotFound)
        ));
        // an absent id is likewise NotFound (indistinguishable from out-of-scope).
        assert!(matches!(
            s.get_page(&t, FactId::new()).await,
            Err(AccessError::NotFound)
        ));
    }

    #[tokio::test]
    async fn get_page_owner_reads_private() {
        let (s, ids) = seeded().await;
        let t = Token::owner();
        assert_eq!(s.get_page(&t, ids[0]).await.unwrap().title, "a");
    }

    #[tokio::test]
    async fn get_guide_hides_ungranted_category() {
        let (s, _) = seeded().await;
        let t = Token::consumer("teammate", [cat("deploy")]);
        // granted → guide built from the visible "deploy" page(s).
        let guide = s.get_guide(&t, &cat("deploy")).await.unwrap();
        assert!(guide.contains("body of b"));
        assert!(!guide.contains("body of a")); // "a" is Private → excluded
                                               // not granted → NotFound (never reveal the category exists).
        assert!(matches!(
            s.get_guide(&t, &cat("workstyle")).await,
            Err(AccessError::NotFound)
        ));
    }

    #[tokio::test]
    async fn get_guide_notfound_when_granted_but_nothing_visible() {
        // A category the consumer is granted but whose only fact is Private has no
        // visible page. `list_categories` omits it, so `get_guide` must agree —
        // NotFound, not an empty Ok("") — or the two tools disagree on reachability.
        let svc = Arc::new(KnowledgeService::new(Arc::new(InMemoryStore::default())));
        svc.upsert(fact("x", Visibility::Private, Some("secret")))
            .await
            .unwrap();
        let s = KnowledgeSharing::new(svc);
        let t = Token::consumer("teammate", [cat("secret")]);
        assert!(s.list_categories(&t).await.unwrap().is_empty());
        assert!(matches!(
            s.get_guide(&t, &cat("secret")).await,
            Err(AccessError::NotFound)
        ));
    }

    /// ⓐ end-to-end: the owner self-reference path (no token) runs the full tool
    /// surface against the real store.
    #[tokio::test]
    async fn owner_self_reference_end_to_end() {
        let (s, _) = seeded().await;
        let t = Token::owner();
        let cats = s.list_categories(&t).await.unwrap();
        assert!(cats.contains(&"deploy".to_string()));
        let results = s.search_knowledge(&t, "body", 10).await.unwrap();
        assert!(!results.is_empty());
        let page = s.get_page(&t, results[0].id).await.unwrap();
        assert!(!page.body.is_empty());
        // owner sees both deploy pages ("a" Private + "b" Shared) in the guide,
        // in stable title order (## a before ## b).
        let guide = s.get_guide(&t, &cat("deploy")).await.unwrap();
        assert!(guide.find("## a").unwrap() < guide.find("## b").unwrap());
    }
}
