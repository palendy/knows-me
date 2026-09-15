//! Confluence connector (Server / Data Center, Personal Access Token).
//!
//! What "mine" means here (BR-C1): pages the owner **created or edited** —
//! CQL `contributor = currentUser()`. Meeting notes, design docs and how-tos
//! the owner wrote are where their working knowledge is written down; pages
//! they merely read are not about them.
//!
//! Behavior:
//!  - Without the `atlassian-http` feature the connector is a safe no-op
//!    skeleton (the crate builds & tests fully offline).
//!  - With it, `sync` runs one CQL search ordered by `lastModified` ascending,
//!    then fetches each page's rendered body (`body.view`) and flattens it to
//!    text. The cursor is the newest `version.when` processed; the next pass
//!    queries from a day before that and skips anything not newer (see
//!    [`super::atlassian::window_start`] for why a day).
//!  - `external_id` is `{page id}:v{version}`, so an edited page is collected
//!    again as a new version while an untouched one is not.
//!  - A page the PAT cannot read (403 — restricted document) is skipped with a
//!    log line; it must not fail the whole sync, and it must not stall the
//!    cursor either.
//!  - In-house Confluence is reached through a read-only **mirror** synced
//!    nightly; API calls go to `base_url` (the mirror) while links use
//!    `web_base_url` (the real server) when the owner set one.
//!  - Missing credentials → `AppError::External("reauth required: confluence")`
//!    so the UI can prompt (re)connection (BR-C3).

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

use super::atlassian::{
    head_chars, parse_atlassian_time, str_at, window_start, AtlassianCreds, MAX_CHARS_PER_ITEM,
};
use crate::core::error::Result;
use crate::core::traits::{Connector, CredentialStore, ProgressReporter};
use crate::core::types::{Cursor, RawItem, SourceKind};
use std::sync::Arc;

/// Pages per sync pass. Each page is a separate GET on a mirror that allows
/// ~15 calls a minute per account, and each collected page then costs an LLM
/// round trip in Processing — ten keeps one press of "collect" bounded and
/// under the rate limit (1 search + 10 fetches).
pub const MAX_PAGES_PER_SYNC: usize = 10;

/// Search page size (how many hits one CQL call returns).
#[cfg(feature = "atlassian-http")]
const SEARCH_LIMIT: usize = 25;

pub struct ConfluenceConnector {
    credentials: Arc<dyn CredentialStore>,
}

impl ConfluenceConnector {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }

    #[cfg_attr(not(feature = "atlassian-http"), allow(dead_code))]
    async fn creds(&self) -> Result<AtlassianCreds> {
        let cred = self.credentials.load(SourceKind::Confluence).await?;
        super::atlassian::creds_from(cred.as_ref(), "confluence")
    }
}

#[async_trait]
impl Connector for ConfluenceConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Confluence
    }

    #[cfg(not(feature = "atlassian-http"))]
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        Ok((Vec::new(), cursor.unwrap_or_default()))
    }

    #[cfg(feature = "atlassian-http")]
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        let creds = self.creds().await?;
        http::sync(&creds, cursor, progress).await
    }

    fn supports_manual(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (offline-testable)
// ---------------------------------------------------------------------------

/// The CQL for one sync pass: my pages, oldest edit first, optionally from a
/// day before the cursor. `author` is not a CQL field — `contributor` is.
pub fn build_cql(since: Option<&DateTime<FixedOffset>>) -> String {
    let mut cql = String::from("contributor = currentUser() AND type = page");
    if let Some(t) = since {
        cql.push_str(&format!(" AND lastModified >= \"{}\"", window_start(t)));
    }
    cql.push_str(" ORDER BY lastModified ASC");
    cql
}

/// The CQL used at connect time to count what the PAT can reach.
pub const VERIFY_CQL: &str = "contributor = currentUser() AND type = page";

/// `version.when` of a content object, if present and parseable.
pub fn page_modified(page: &Value) -> Option<DateTime<FixedOffset>> {
    parse_atlassian_time(str_at(page, &["version", "when"]))
}

/// `{id}:v{version}` — a new version of a page is new material.
pub fn external_id(page: &Value) -> String {
    let id = str_at(page, &["id"]);
    let version = page["version"]["number"].as_u64().unwrap_or(0);
    format!("{id}:v{version}")
}

/// The link the owner clicks: web base + `_links.webui`, or the standard
/// `viewpage.action` path when the server omitted it.
pub fn page_url(link_base: &str, page: &Value) -> String {
    let webui = str_at(page, &["_links", "webui"]);
    if !webui.is_empty() {
        let sep = if webui.starts_with('/') { "" } else { "/" };
        return format!("{link_base}{sep}{webui}");
    }
    format!(
        "{link_base}/pages/viewpage.action?pageId={}",
        str_at(page, &["id"])
    )
}

/// Flatten a fully-expanded content object (`body.view, space, version,
/// ancestors, metadata.labels`) into the text the extraction pipeline reads.
/// Returns `None` when the body is empty — the mirror answers a restricted
/// page with 200 and an empty body, and a title alone teaches nothing.
pub fn render_page(link_base: &str, page: &Value) -> Option<String> {
    let body = super::atlassian::html_to_text(str_at(page, &["body", "view", "value"]));
    if body.is_empty() {
        return None;
    }
    let title = str_at(page, &["title"]);
    let space_name = str_at(page, &["space", "name"]);
    let space_key = str_at(page, &["space", "key"]);
    let path: Vec<&str> = page["ancestors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| str_at(a, &["title"]))
        .filter(|t| !t.is_empty())
        .collect();
    let labels: Vec<&str> = page["metadata"]["labels"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| str_at(l, &["name"]))
        .filter(|n| !n.is_empty())
        .collect();
    let when = str_at(page, &["version", "when"]);
    let by = {
        let d = str_at(page, &["version", "by", "displayName"]);
        if d.is_empty() {
            str_at(page, &["version", "by", "username"])
        } else {
            d
        }
    };
    let version = page["version"]["number"].as_u64().unwrap_or(0);

    let mut out = format!("[Confluence 페이지] {title}\n");
    if !space_name.is_empty() || !space_key.is_empty() {
        out.push_str(&format!("공간: {space_name} ({space_key})\n"));
    }
    if !path.is_empty() {
        out.push_str(&format!("경로: {}\n", path.join(" > ")));
    }
    out.push_str(&format!("URL: {}\n", page_url(link_base, page)));
    out.push_str(&format!("최종 수정: {when} · {by} (v{version})\n"));
    if !labels.is_empty() {
        out.push_str(&format!("라벨: {}\n", labels.join(", ")));
    }
    out.push('\n');
    out.push_str(&body);
    Some(head_chars(&out, MAX_CHARS_PER_ITEM))
}

// ---------------------------------------------------------------------------
// HTTP (feature `atlassian-http`)
// ---------------------------------------------------------------------------

#[cfg(feature = "atlassian-http")]
pub mod http {
    use chrono::{DateTime, FixedOffset, Utc};
    use serde_json::Value;

    use super::super::atlassian::http::{client, get_json};
    use super::super::atlassian::{parse_atlassian_time, str_at, AtlassianCreds};
    use super::{
        build_cql, external_id, page_modified, render_page, MAX_PAGES_PER_SYNC, SEARCH_LIMIT,
        VERIFY_CQL,
    };
    use crate::core::error::{AppError, Result};
    use crate::core::traits::ProgressReporter;
    use crate::core::types::{Cursor, RawItem, SourceKind};

    const SERVICE: &str = "Confluence";

    /// Connect-time handshake: who the PAT is, and how many of "my" pages the
    /// search can see (`None` when the server did not report a total).
    pub async fn verify(creds: &AtlassianCreds) -> Result<(String, Option<u64>)> {
        let client = client(&creds.pat)?;
        let me = get_json(
            &client,
            &format!("{}/rest/api/user/current", creds.base_url),
            &[],
            SERVICE,
            "사용자 확인",
        )
        .await?;
        let username = str_at(&me, &["username"]);
        if username.is_empty() || str_at(&me, &["type"]) == "anonymous" {
            return Err(AppError::External(
                "Confluence가 PAT를 인식하지 못했습니다 (익명 사용자로 응답). 토큰과 서버 주소를 확인하세요."
                    .into(),
            ));
        }
        let display = {
            let d = str_at(&me, &["displayName"]);
            if d.is_empty() {
                username.to_string()
            } else {
                d.to_string()
            }
        };

        // Reach: a count, not the pages. A failure here is not a connect
        // failure — the PAT is valid; the count is a courtesy.
        let count = get_json(
            &client,
            &format!("{}/rest/api/content/search", creds.base_url),
            &[("cql", VERIFY_CQL.to_string()), ("limit", "1".to_string())],
            SERVICE,
            "검색",
        )
        .await
        .ok()
        .and_then(|v| v["totalSize"].as_u64());
        Ok((display, count))
    }

    /// One bounded pass: search my pages from the cursor window, fetch bodies,
    /// return items + the newest edit time as the next cursor.
    pub async fn sync(
        creds: &AtlassianCreds,
        cursor: Option<Cursor>,
        progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        let client = client(&creds.pat)?;
        let since: Option<DateTime<FixedOffset>> =
            cursor.as_ref().and_then(|c| parse_atlassian_time(&c.0));
        let cql = build_cql(since.as_ref());

        let mut items = Vec::new();
        let mut newest = since;
        let mut start = 0usize;
        let mut total_hint = 0usize;
        let mut skipped_by_cursor = 0usize;
        let mut unreadable = 0usize;

        'outer: loop {
            let page: Value = get_json(
                &client,
                &format!("{}/rest/api/content/search", creds.base_url),
                &[
                    ("cql", cql.clone()),
                    ("expand", "version".to_string()),
                    ("limit", SEARCH_LIMIT.to_string()),
                    ("start", start.to_string()),
                ],
                SERVICE,
                "검색",
            )
            .await?;
            if let Some(t) = page["totalSize"].as_u64() {
                total_hint = t as usize;
            }
            let results = page["results"].as_array().cloned().unwrap_or_default();
            if results.is_empty() {
                break;
            }

            for hit in &results {
                let Some(when) = page_modified(hit) else {
                    continue;
                };
                if since.is_some_and(|s| when <= s) {
                    skipped_by_cursor += 1;
                    continue;
                }
                let id = str_at(hit, &["id"]).to_string();
                if id.is_empty() {
                    continue;
                }

                // Body fetch. A restricted page answers 403 — skip it, keep
                // going, and still move the cursor past it.
                let full = match get_json(
                    &client,
                    &format!("{}/rest/api/content/{id}", creds.base_url),
                    &[(
                        "expand",
                        "body.view,space,version,ancestors,metadata.labels".to_string(),
                    )],
                    SERVICE,
                    "페이지 조회",
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        unreadable += 1;
                        eprintln!("[confluence] page {id} skipped: {e}");
                        if newest.is_none_or(|n| when > n) {
                            newest = Some(when);
                        }
                        continue;
                    }
                };

                if let Some(text) = render_page(creds.link_base(), &full) {
                    items.push(RawItem {
                        source: SourceKind::Confluence,
                        external_id: external_id(&full),
                        collected_at: when.with_timezone(&Utc),
                        text: Some(text),
                        image_png: None,
                    });
                }
                if newest.is_none_or(|n| when > n) {
                    newest = Some(when);
                }
                progress.progress(SourceKind::Confluence, items.len(), total_hint);
                if items.len() >= MAX_PAGES_PER_SYNC {
                    break 'outer;
                }
            }

            let has_next = !page["_links"]["next"].is_null();
            if !has_next && results.len() < SEARCH_LIMIT {
                break;
            }
            start += results.len();
        }

        let next = newest
            .map(|t| Cursor(t.to_rfc3339()))
            .or(cursor)
            .unwrap_or_default();
        eprintln!(
            "[confluence] sync: total={total_hint} skipped_by_cursor={skipped_by_cursor} unreadable={unreadable} collected={}",
            items.len()
        );
        Ok((items, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Credential;
    use serde_json::json;

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

    fn sample_page() -> Value {
        json!({
            "id": "123456",
            "title": "배포 절차",
            "space": { "key": "ENG", "name": "Engineering" },
            "version": {
                "number": 7,
                "when": "2026-09-15T10:22:33.000+09:00",
                "by": { "username": "hong", "displayName": "홍길동" }
            },
            "ancestors": [ { "title": "운영" }, { "title": "런북" } ],
            "metadata": { "labels": { "results": [ { "name": "deploy" }, { "name": "runbook" } ] } },
            "body": { "view": { "value": "<p>먼저 <b>staging</b>에 올린다.</p><ul><li>PR은 squash</li></ul>" } },
            "_links": { "webui": "/display/ENG/deploy" }
        })
    }

    #[tokio::test]
    async fn creds_error_when_absent() {
        let conn = ConfluenceConnector::new(Arc::new(FakeCreds(None)));
        assert_eq!(conn.id(), SourceKind::Confluence);
        let err = conn.creds().await.unwrap_err();
        assert!(err.to_string().contains("reauth required: confluence"));
    }

    #[tokio::test]
    async fn creds_read_stored_values() {
        let cred = Credential(json!({ "base_url": "https://c.example.com/", "pat": "tok" }));
        let conn = ConfluenceConnector::new(Arc::new(FakeCreds(Some(cred))));
        let c = conn.creds().await.unwrap();
        assert_eq!(c.base_url, "https://c.example.com");
        assert_eq!(c.pat, "tok");
    }

    #[test]
    fn cql_windows_from_a_day_before_the_cursor() {
        assert_eq!(
            build_cql(None),
            "contributor = currentUser() AND type = page ORDER BY lastModified ASC"
        );
        let t = parse_atlassian_time("2026-09-15T10:22:33.000+09:00").unwrap();
        assert_eq!(
            build_cql(Some(&t)),
            "contributor = currentUser() AND type = page AND lastModified >= \"2026-09-14 10:22\" ORDER BY lastModified ASC"
        );
    }

    #[test]
    fn external_id_changes_with_the_version() {
        assert_eq!(external_id(&sample_page()), "123456:v7");
        let mut later = sample_page();
        later["version"]["number"] = json!(8);
        assert_ne!(external_id(&later), external_id(&sample_page()));
    }

    #[test]
    fn page_url_prefers_webui_and_falls_back_to_viewpage() {
        assert_eq!(
            page_url("https://conf.example.com", &sample_page()),
            "https://conf.example.com/display/ENG/deploy"
        );
        let mut no_links = sample_page();
        no_links["_links"] = json!({});
        assert_eq!(
            page_url("https://conf.example.com", &no_links),
            "https://conf.example.com/pages/viewpage.action?pageId=123456"
        );
    }

    #[test]
    fn render_page_produces_readable_text_with_metadata() {
        let text = render_page("https://conf.example.com", &sample_page()).unwrap();
        assert!(text.starts_with("[Confluence 페이지] 배포 절차\n"));
        assert!(text.contains("공간: Engineering (ENG)"));
        assert!(text.contains("경로: 운영 > 런북"));
        assert!(text.contains("URL: https://conf.example.com/display/ENG/deploy"));
        assert!(text.contains("최종 수정: 2026-09-15T10:22:33.000+09:00 · 홍길동 (v7)"));
        assert!(text.contains("라벨: deploy, runbook"));
        assert!(text.ends_with("먼저 staging에 올린다.\nPR은 squash"));
    }

    #[test]
    fn render_page_skips_an_empty_body() {
        // The mirror answers a restricted page with 200 + empty body.
        let mut restricted = sample_page();
        restricted["body"]["view"]["value"] = json!("");
        assert!(render_page("https://c", &restricted).is_none());
    }

    #[test]
    fn modified_time_comes_from_version_when() {
        let t = page_modified(&sample_page()).unwrap();
        assert_eq!(t.to_rfc3339(), "2026-09-15T10:22:33+09:00");
        assert!(page_modified(&json!({})).is_none());
    }

    #[cfg(not(feature = "atlassian-http"))]
    #[tokio::test]
    async fn skeleton_is_safe_noop() {
        let conn = ConfluenceConnector::new(Arc::new(FakeCreds(None)));
        let (items, c) = conn
            .sync(Some(Cursor("x".into())), &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert!(items.is_empty());
        assert_eq!(c, Cursor("x".into()));
    }
}
