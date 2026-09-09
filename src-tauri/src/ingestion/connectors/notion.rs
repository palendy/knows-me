//! Notion connector.
//!
//! Auth model (Q3=A / BR-C1..C4): an **Internal Integration Token** stored via
//! U1 [`CredentialStore`]. Notion scopes access to the pages an integration is
//! explicitly connected to, so "collect everything" means: the owner connects
//! the integration to their top-level page(s) once (access is inherited by the
//! subtree), and this connector then walks every page the token can see via
//! `POST /v1/search` — no per-page config.
//!
//! Behavior:
//!  - Without the `notion-http` feature the connector is a safe no-op skeleton
//!    (the crate builds & tests fully offline).
//!  - With `notion-http`, `sync` pages through `search`, extracts each page's
//!    block text, and maps it to a [`RawItem`]. `cursor` is the newest
//!    `last_edited_time` seen so re-syncs only pull pages edited since.
//!  - A missing/empty token → `AppError::External("reauth required: notion")`
//!    so the UI can prompt (re)connection (BR-C3).
//!  - Bodies MUST pass through Processing's masking before any cloud call
//!    (BR-C4 / BR-K1) — this connector only collects raw text.

use async_trait::async_trait;

use crate::core::error::Result;
use crate::core::traits::{Connector, CredentialStore};
use crate::core::types::{Cursor, RawItem, SourceKind};
use std::sync::Arc;

/// Notion connector. Holds the credential store it loads the integration token
/// from at sync time (never cached, never in code — BR-C2).
pub struct NotionConnector {
    credentials: Arc<dyn CredentialStore>,
}

impl NotionConnector {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }

    /// Load the integration token from the stored credential (`{"token": "..."}`),
    /// erroring in the shape the UI expects when it is absent/empty.
    #[cfg_attr(not(feature = "notion-http"), allow(dead_code))]
    async fn token(&self) -> Result<String> {
        let cred = self.credentials.load(SourceKind::Notion).await?;
        let token = cred
            .as_ref()
            .and_then(|c| c.0.get("token"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match token {
            Some(t) => Ok(t.to_string()),
            None => Err(crate::core::error::AppError::External(
                "reauth required: notion".into(),
            )),
        }
    }
}

#[async_trait]
impl Connector for NotionConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Notion
    }

    #[cfg(not(feature = "notion-http"))]
    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        // Skeleton: no new items, cursor unchanged (safe no-op so the
        // orchestrator and idempotency logic can be exercised offline).
        Ok((Vec::new(), cursor.unwrap_or_default()))
    }

    #[cfg(feature = "notion-http")]
    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        let token = self.token().await?;
        http::sync(&token, cursor).await
    }

    fn supports_manual(&self) -> bool {
        false
    }
}

/// Real Notion HTTP sync (feature `notion-http`).
#[cfg(feature = "notion-http")]
pub(crate) mod http {
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};

    use crate::core::error::{AppError, Result};
    use crate::core::types::{Cursor, RawItem, SourceKind};

    const API_BASE: &str = "https://api.notion.com/v1";
    const NOTION_VERSION: &str = "2022-06-28";
    /// Cap pages per sync so a huge workspace doesn't stall a single run; the
    /// cursor advances so the next run continues (mirrors the Session connector).
    const MAX_PAGES_PER_SYNC: usize = 100;

    fn client(token: &str) -> Result<reqwest::Client> {
        use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| AppError::External(format!("notion auth header: {e}")))?;
        auth.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth);
        headers.insert("Notion-Version", HeaderValue::from_static(NOTION_VERSION));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| AppError::External(format!("notion client: {e}")))
    }

    /// Verify a token at connect time and report how many pages it can actually
    /// reach.
    ///
    /// Two distinct checks, because a valid token that can see nothing is the
    /// most common Notion pitfall (the integration must be *connected* to pages,
    /// not just created):
    ///  1. `GET /v1/users/me` — is the token itself valid? A bad token errors.
    ///  2. `POST /v1/search` (one page) — how many pages/DBs are shared with it?
    ///     Returns that count so the UI can warn when it's 0.
    pub async fn verify(token: &str) -> Result<usize> {
        let client = client(token)?;

        // 1. Token validity.
        let me = client
            .get(format!("{API_BASE}/users/me"))
            .send()
            .await
            .map_err(|e| AppError::External(format!("notion 연결 확인 실패: {e}")))?;
        if !me.status().is_success() {
            let status = me.status();
            let body: Value = me.json().await.unwrap_or_else(|_| json!({}));
            let msg = body["message"].as_str().unwrap_or("알 수 없는 오류");
            return Err(AppError::External(if status.as_u16() == 401 {
                format!("유효하지 않은 토큰입니다 (401): {msg}")
            } else {
                format!("Notion 연결 확인 실패 ({status}): {msg}")
            }));
        }

        // 2. Reachable page/DB count (one search page is enough to tell 0 apart
        //    from "some"; `has_more` means there are more than the first batch).
        let resp = client
            .post(format!("{API_BASE}/search"))
            .json(&json!({ "page_size": 100 }))
            .send()
            .await
            .map_err(|e| AppError::External(format!("notion 접근 범위 확인 실패: {e}")))?;
        if !resp.status().is_success() {
            // Token is valid but search failed — treat the reach as unknown (0)
            // rather than failing the whole connect.
            return Ok(0);
        }
        let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        let count = body["results"].as_array().map(|a| a.len()).unwrap_or(0);
        Ok(count)
    }

    /// Page through `search` (oldest edits first) and collect page text. Returns
    /// the items plus the newest `last_edited_time` as the next cursor.
    pub async fn sync(token: &str, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        let client = client(token)?;

        // The cursor is the last `last_edited_time` we processed; only pages
        // edited strictly after it are new work.
        let since: Option<DateTime<Utc>> = cursor
            .as_ref()
            .and_then(|c| DateTime::parse_from_rfc3339(&c.0).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let mut items = Vec::new();
        let mut newest = since;
        let mut start_cursor: Option<String> = None;
        let mut total_seen = 0usize;
        let mut skipped_by_cursor = 0usize;

        'outer: loop {
            // No object filter: take pages AND databases the integration can
            // see. Filtering to "page" alone hides workspaces where only a
            // database is shared, which reads as "nothing to collect".
            let mut body = json!({
                "sort": { "direction": "ascending", "timestamp": "last_edited_time" },
                "page_size": 100,
            });
            if let Some(sc) = &start_cursor {
                body["start_cursor"] = json!(sc);
            }

            let resp = client
                .post(format!("{API_BASE}/search"))
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::External(format!("notion search: {e}")))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(AppError::External(format!(
                    "notion search {status}: {text}"
                )));
            }
            let page: Value = resp
                .json()
                .await
                .map_err(|e| AppError::External(format!("notion search decode: {e}")))?;

            total_seen += page["results"].as_array().map(|a| a.len()).unwrap_or(0);

            for obj in page["results"].as_array().into_iter().flatten() {
                let edited = obj["last_edited_time"]
                    .as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));
                // Skip anything not newer than the cursor (idempotent-ish; the
                // orchestrator's seen-set is the real dedup gate).
                if let (Some(since), Some(edited)) = (since, edited) {
                    if edited <= since {
                        skipped_by_cursor += 1;
                        continue;
                    }
                }
                let Some(id) = obj["id"].as_str() else {
                    continue;
                };
                let collected_at = edited.unwrap_or_else(Utc::now);
                let text = page_text(&client, id).await?;
                items.push(RawItem {
                    source: SourceKind::Notion,
                    external_id: id.to_string(),
                    collected_at,
                    text: (!text.trim().is_empty()).then_some(text),
                    image_png: None,
                });
                if newest.is_none() || edited > newest {
                    newest = edited;
                }
                if items.len() >= MAX_PAGES_PER_SYNC {
                    break 'outer;
                }
            }

            match page["next_cursor"].as_str() {
                Some(next) if page["has_more"].as_bool() == Some(true) => {
                    start_cursor = Some(next.to_string());
                }
                _ => break,
            }
        }

        // Cursor advances to the newest edit we processed; unchanged if nothing new.
        let next = newest
            .map(|dt| Cursor(dt.to_rfc3339()))
            .or(cursor)
            .unwrap_or_default();
        // One-line summary is enough for ops: how many the search saw, how many
        // the cursor skipped, and how many we actually pulled.
        eprintln!(
            "[notion] sync: search={total_seen} skipped_by_cursor={skipped_by_cursor} collected={}",
            items.len()
        );
        Ok((items, next))
    }

    /// Fetch a page's top-level block children and flatten their rich text.
    async fn page_text(client: &reqwest::Client, page_id: &str) -> Result<String> {
        let resp = client
            .get(format!(
                "{API_BASE}/blocks/{page_id}/children?page_size=100"
            ))
            .send()
            .await
            .map_err(|e| AppError::External(format!("notion blocks: {e}")))?;
        if !resp.status().is_success() {
            // A page we can't read (shared partially) shouldn't fail the whole
            // sync — treat it as empty text.
            eprintln!(
                "[notion.sync] blocks {page_id} not readable: {}",
                resp.status()
            );
            return Ok(String::new());
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("notion blocks decode: {e}")))?;

        let mut out = String::new();
        for block in body["results"].as_array().into_iter().flatten() {
            // Each block type nests its content under its type name, e.g.
            // { "type": "paragraph", "paragraph": { "rich_text": [...] } }.
            let Some(ty) = block["type"].as_str() else {
                continue;
            };
            if let Some(rich) = block[ty]["rich_text"].as_array() {
                for span in rich {
                    if let Some(t) = span["plain_text"].as_str() {
                        out.push_str(t);
                    }
                }
                out.push('\n');
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Credential;

    struct FakeCreds(Option<Credential>);
    #[async_trait]
    impl CredentialStore for FakeCreds {
        async fn store(&self, _s: SourceKind, _c: Credential) -> Result<()> {
            Ok(())
        }
        async fn load(&self, _s: SourceKind) -> Result<Option<Credential>> {
            Ok(self.0.clone())
        }
        async fn delete(&self, _s: SourceKind) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn token_errors_when_absent() {
        let conn = NotionConnector::new(Arc::new(FakeCreds(None)));
        assert_eq!(conn.id(), SourceKind::Notion);
        let err = conn.token().await.unwrap_err();
        assert!(err.to_string().contains("reauth required: notion"));
    }

    #[tokio::test]
    async fn token_errors_when_empty() {
        let cred = Credential(serde_json::json!({ "token": "   " }));
        let conn = NotionConnector::new(Arc::new(FakeCreds(Some(cred))));
        assert!(conn.token().await.is_err());
    }

    #[tokio::test]
    async fn token_reads_stored_value() {
        let cred = Credential(serde_json::json!({ "token": "secret_abc" }));
        let conn = NotionConnector::new(Arc::new(FakeCreds(Some(cred))));
        assert_eq!(conn.token().await.unwrap(), "secret_abc");
    }

    #[cfg(not(feature = "notion-http"))]
    #[tokio::test]
    async fn skeleton_is_safe_noop() {
        let conn = NotionConnector::new(Arc::new(FakeCreds(None)));
        let (items, _c) = conn.sync(None).await.unwrap();
        assert_eq!(items.len(), 0);
    }
}
