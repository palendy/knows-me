//! Jira connector (Server / Data Center, Personal Access Token).
//!
//! What "mine" means here (BR-C1): issues the owner is **assignee or
//! reporter** of — JQL `assignee = currentUser() OR reporter = currentUser()`
//! (alpha-agent-v3's default reading of "my issues"). The description plus
//! the comment thread is where decisions and their reasons get written down.
//!
//! Behavior:
//!  - Without the `atlassian-http` feature the connector is a safe no-op
//!    skeleton (the crate builds & tests fully offline).
//!  - With it, `sync` runs one JQL search (`POST /rest/api/2/search`, long
//!    JQL is safer in a body) ordered by `updated` ascending, asking for the
//!    fields it renders — including `comment`, so one call returns the whole
//!    issue. The cursor is the newest `updated` processed; the next pass
//!    queries from a day before that and skips anything not newer.
//!  - `external_id` is `{key}:open` while unresolved and `{key}:{resolution
//!    date}` once resolved, so an issue is collected at most twice: once as
//!    it is being worked (if a pass reaches it then) and once in its final
//!    state with the full discussion. Every status flip re-collecting it
//!    would cost an LLM call per transition for nothing new.
//!  - Missing credentials → `AppError::External("reauth required: jira")`.

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

/// Issues per sync pass. One search call returns them all (bodies and
/// comments included), so the bound is about Processing cost, not the API.
pub const MAX_ISSUES_PER_SYNC: usize = 25;

/// Fields requested from search — everything `render_issue` reads.
pub const FIELDS: &[&str] = &[
    "summary",
    "status",
    "assignee",
    "reporter",
    "issuetype",
    "priority",
    "project",
    "description",
    "labels",
    "created",
    "updated",
    "resolution",
    "resolutiondate",
    "parent",
    "comment",
];

pub struct JiraConnector {
    credentials: Arc<dyn CredentialStore>,
}

impl JiraConnector {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }

    #[cfg_attr(not(feature = "atlassian-http"), allow(dead_code))]
    async fn creds(&self) -> Result<AtlassianCreds> {
        let cred = self.credentials.load(SourceKind::Jira).await?;
        super::atlassian::creds_from(cred.as_ref(), "jira")
    }
}

#[async_trait]
impl Connector for JiraConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Jira
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

/// The JQL used at connect time to count what the PAT can reach.
pub const VERIFY_JQL: &str = "assignee = currentUser() OR reporter = currentUser()";

/// The JQL for one sync pass: my issues, oldest update first, optionally
/// from a day before the cursor.
pub fn build_jql(since: Option<&DateTime<FixedOffset>>) -> String {
    let mut jql = format!("({VERIFY_JQL})");
    if let Some(t) = since {
        jql.push_str(&format!(" AND updated >= \"{}\"", window_start(t)));
    }
    jql.push_str(" ORDER BY updated ASC");
    jql
}

/// `fields.updated`, if present and parseable.
pub fn issue_updated(issue: &Value) -> Option<DateTime<FixedOffset>> {
    parse_atlassian_time(str_at(issue, &["fields", "updated"]))
}

/// `{key}:open` until resolved, then `{key}:{resolutiondate}`.
pub fn external_id(issue: &Value) -> String {
    let key = str_at(issue, &["key"]);
    let resolved = str_at(issue, &["fields", "resolutiondate"]);
    if resolved.is_empty() {
        format!("{key}:open")
    } else {
        format!("{key}:{resolved}")
    }
}

/// `<base>/browse/<KEY>` — Jira's standard issue link.
pub fn issue_url(link_base: &str, key: &str) -> String {
    format!("{link_base}/browse/{key}")
}

fn person(v: &Value) -> &str {
    let d = str_at(v, &["displayName"]);
    if d.is_empty() {
        str_at(v, &["name"])
    } else {
        d
    }
}

/// Flatten an issue (as returned by search with [`FIELDS`]) into the text the
/// extraction pipeline reads. Description is Jira wiki markup and is passed
/// through as-is — it reads fine. Always `Some`: even a bare summary plus
/// status/assignee is a fact about what the owner worked on.
pub fn render_issue(link_base: &str, issue: &Value) -> String {
    let f = &issue["fields"];
    let key = str_at(issue, &["key"]);
    let summary = str_at(f, &["summary"]);

    let mut out = format!("[Jira 이슈] {key} {summary}\n");
    out.push_str(&format!("URL: {}\n", issue_url(link_base, key)));

    let project_name = str_at(f, &["project", "name"]);
    let project_key = str_at(f, &["project", "key"]);
    let mut line = Vec::new();
    if !project_key.is_empty() {
        line.push(format!("프로젝트: {project_name} ({project_key})"));
    }
    for (label, path) in [
        ("유형", ["issuetype", "name"]),
        ("상태", ["status", "name"]),
        ("우선순위", ["priority", "name"]),
    ] {
        let v = str_at(f, &path);
        if !v.is_empty() {
            line.push(format!("{label}: {v}"));
        }
    }
    if !line.is_empty() {
        out.push_str(&line.join(" · "));
        out.push('\n');
    }

    let assignee = person(&f["assignee"]);
    let reporter = person(&f["reporter"]);
    if !assignee.is_empty() || !reporter.is_empty() {
        out.push_str(&format!(
            "담당자: {} · 보고자: {}\n",
            if assignee.is_empty() {
                "(없음)"
            } else {
                assignee
            },
            if reporter.is_empty() {
                "(없음)"
            } else {
                reporter
            }
        ));
    }

    let created = str_at(f, &["created"]);
    let updated = str_at(f, &["updated"]);
    if !created.is_empty() || !updated.is_empty() {
        let mut when = format!("생성: {created} · 수정: {updated}");
        let resolution = str_at(f, &["resolution", "name"]);
        if !resolution.is_empty() {
            when.push_str(&format!(
                " · 해결: {resolution} ({})",
                str_at(f, &["resolutiondate"])
            ));
        }
        out.push_str(&when);
        out.push('\n');
    }

    let labels: Vec<&str> = f["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !labels.is_empty() {
        out.push_str(&format!("라벨: {}\n", labels.join(", ")));
    }
    let parent_key = str_at(f, &["parent", "key"]);
    if !parent_key.is_empty() {
        out.push_str(&format!(
            "상위: {parent_key} {}\n",
            str_at(f, &["parent", "fields", "summary"])
        ));
    }

    let description = str_at(f, &["description"]).trim();
    if !description.is_empty() {
        out.push_str("\n설명:\n");
        out.push_str(description);
        out.push('\n');
    }

    let comments: Vec<&Value> = f["comment"]["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    if !comments.is_empty() {
        out.push_str(&format!("\n댓글 {}건:\n", comments.len()));
        for c in comments {
            let body = str_at(c, &["body"]).trim();
            if body.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "- {} ({}):\n  {}\n",
                person(&c["author"]),
                str_at(c, &["created"]),
                body.replace('\n', "\n  ")
            ));
        }
    }

    head_chars(out.trim_end(), MAX_CHARS_PER_ITEM)
}

// ---------------------------------------------------------------------------
// HTTP (feature `atlassian-http`)
// ---------------------------------------------------------------------------

#[cfg(feature = "atlassian-http")]
pub mod http {
    use chrono::{DateTime, FixedOffset, Utc};
    use serde_json::{json, Value};

    use super::super::atlassian::http::{client, get_json, post_json};
    use super::super::atlassian::{parse_atlassian_time, str_at, AtlassianCreds};
    use super::{
        build_jql, external_id, issue_updated, render_issue, FIELDS, MAX_ISSUES_PER_SYNC,
        VERIFY_JQL,
    };
    use crate::core::error::{AppError, Result};
    use crate::core::traits::ProgressReporter;
    use crate::core::types::{Cursor, RawItem, SourceKind};

    const SERVICE: &str = "Jira";

    /// Connect-time handshake: who the PAT is, and how many of "my" issues
    /// the search can see (`None` if the count call failed).
    pub async fn verify(creds: &AtlassianCreds) -> Result<(String, Option<u64>)> {
        let client = client(&creds.pat)?;
        let me = get_json(
            &client,
            &format!("{}/rest/api/2/myself", creds.base_url),
            &[],
            SERVICE,
            "사용자 확인",
        )
        .await?;
        let name = str_at(&me, &["name"]);
        if name.is_empty() {
            return Err(AppError::External(
                "Jira가 PAT를 인식하지 못했습니다 (사용자 정보 없음). 토큰과 서버 주소를 확인하세요."
                    .into(),
            ));
        }
        let display = {
            let d = str_at(&me, &["displayName"]);
            if d.is_empty() {
                name.to_string()
            } else {
                d.to_string()
            }
        };

        let count = post_json(
            &client,
            &format!("{}/rest/api/2/search", creds.base_url),
            &json!({ "jql": VERIFY_JQL, "maxResults": 0, "fields": ["summary"] }),
            SERVICE,
            "검색",
        )
        .await
        .ok()
        .and_then(|v| v["total"].as_u64());
        Ok((display, count))
    }

    /// One bounded pass: search my issues from the cursor window, return
    /// items + the newest update time as the next cursor.
    pub async fn sync(
        creds: &AtlassianCreds,
        cursor: Option<Cursor>,
        progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        let client = client(&creds.pat)?;
        let since: Option<DateTime<FixedOffset>> =
            cursor.as_ref().and_then(|c| parse_atlassian_time(&c.0));
        let jql = build_jql(since.as_ref());

        let mut items = Vec::new();
        let mut newest = since;
        let mut start_at = 0usize;
        let mut total = 0usize;
        let mut skipped_by_cursor = 0usize;

        'outer: loop {
            let page: Value = post_json(
                &client,
                &format!("{}/rest/api/2/search", creds.base_url),
                &json!({
                    "jql": jql,
                    "startAt": start_at,
                    "maxResults": MAX_ISSUES_PER_SYNC,
                    "fields": FIELDS,
                }),
                SERVICE,
                "검색",
            )
            .await?;
            let issues = page["issues"].as_array().cloned().unwrap_or_default();
            if issues.is_empty() {
                break;
            }
            total = page["total"].as_u64().unwrap_or(0) as usize;

            for issue in &issues {
                let Some(when) = issue_updated(issue) else {
                    continue;
                };
                if since.is_some_and(|s| when <= s) {
                    skipped_by_cursor += 1;
                    continue;
                }
                items.push(RawItem {
                    source: SourceKind::Jira,
                    external_id: external_id(issue),
                    collected_at: when.with_timezone(&Utc),
                    text: Some(render_issue(creds.link_base(), issue)),
                    image_png: None,
                });
                if newest.is_none_or(|n| when > n) {
                    newest = Some(when);
                }
                progress.progress(SourceKind::Jira, items.len(), total);
                if items.len() >= MAX_ISSUES_PER_SYNC {
                    break 'outer;
                }
            }

            start_at += issues.len();
            if start_at >= total {
                break;
            }
        }

        let next = newest
            .map(|t| Cursor(t.to_rfc3339()))
            .or(cursor)
            .unwrap_or_default();
        eprintln!(
            "[jira] sync: total={total} skipped_by_cursor={skipped_by_cursor} collected={}",
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

    fn sample_issue() -> Value {
        json!({
            "key": "ALPHA-123",
            "fields": {
                "summary": "로그인 타임아웃 조정",
                "status": { "name": "In Progress" },
                "assignee": { "name": "hong", "displayName": "홍길동" },
                "reporter": { "name": "kim", "displayName": "김지원" },
                "issuetype": { "name": "Task" },
                "priority": { "name": "High" },
                "project": { "key": "ALPHA", "name": "Alpha Agent" },
                "description": "세션 만료가 너무 짧다.\n30분으로 늘린다.",
                "labels": ["auth", "backend"],
                "created": "2026-09-01T09:00:00.000+0900",
                "updated": "2026-09-15T10:22:33.000+0900",
                "resolution": null,
                "resolutiondate": null,
                "parent": { "key": "ALPHA-100", "fields": { "summary": "인증 개선" } },
                "comment": { "comments": [
                    { "author": { "displayName": "김지원" }, "created": "2026-09-02T10:00:00.000+0900", "body": "기존 값은 5분" },
                    { "author": { "name": "hong" }, "created": "2026-09-03T10:00:00.000+0900", "body": "30분으로 확정\n배포는 다음 주" }
                ] }
            }
        })
    }

    #[tokio::test]
    async fn creds_error_when_absent() {
        let conn = JiraConnector::new(Arc::new(FakeCreds(None)));
        assert_eq!(conn.id(), SourceKind::Jira);
        let err = conn.creds().await.unwrap_err();
        assert!(err.to_string().contains("reauth required: jira"));
    }

    #[test]
    fn jql_windows_from_a_day_before_the_cursor() {
        assert_eq!(
            build_jql(None),
            "(assignee = currentUser() OR reporter = currentUser()) ORDER BY updated ASC"
        );
        let t = parse_atlassian_time("2026-09-15T10:22:33.000+0900").unwrap();
        assert_eq!(
            build_jql(Some(&t)),
            "(assignee = currentUser() OR reporter = currentUser()) AND updated >= \"2026-09-14 10:22\" ORDER BY updated ASC"
        );
    }

    #[test]
    fn external_id_flips_once_on_resolution() {
        assert_eq!(external_id(&sample_issue()), "ALPHA-123:open");
        let mut done = sample_issue();
        done["fields"]["resolutiondate"] = json!("2026-09-20T18:00:00.000+0900");
        assert_eq!(external_id(&done), "ALPHA-123:2026-09-20T18:00:00.000+0900");
        // A later status change without a new resolution is not new material.
        let mut touched = done.clone();
        touched["fields"]["updated"] = json!("2026-09-21T09:00:00.000+0900");
        assert_eq!(external_id(&touched), external_id(&done));
    }

    #[test]
    fn render_issue_carries_description_and_comments() {
        let text = render_issue("https://jira.example.com", &sample_issue());
        assert!(text.starts_with("[Jira 이슈] ALPHA-123 로그인 타임아웃 조정\n"));
        assert!(text.contains("URL: https://jira.example.com/browse/ALPHA-123"));
        assert!(text.contains(
            "프로젝트: Alpha Agent (ALPHA) · 유형: Task · 상태: In Progress · 우선순위: High"
        ));
        assert!(text.contains("담당자: 홍길동 · 보고자: 김지원"));
        assert!(text
            .contains("생성: 2026-09-01T09:00:00.000+0900 · 수정: 2026-09-15T10:22:33.000+0900\n"));
        assert!(!text.contains("해결:"));
        assert!(text.contains("라벨: auth, backend"));
        assert!(text.contains("상위: ALPHA-100 인증 개선"));
        assert!(text.contains("설명:\n세션 만료가 너무 짧다.\n30분으로 늘린다.\n"));
        assert!(text.contains("댓글 2건:\n- 김지원 (2026-09-02T10:00:00.000+0900):\n  기존 값은 5분\n- hong (2026-09-03T10:00:00.000+0900):\n  30분으로 확정\n  배포는 다음 주"));
    }

    #[test]
    fn render_issue_tolerates_a_bare_issue() {
        let text = render_issue(
            "https://j",
            &json!({ "key": "X-1", "fields": { "summary": "s" } }),
        );
        assert_eq!(text, "[Jira 이슈] X-1 s\nURL: https://j/browse/X-1");
    }

    #[test]
    fn updated_time_comes_from_fields_updated() {
        let t = issue_updated(&sample_issue()).unwrap();
        assert_eq!(t.to_rfc3339(), "2026-09-15T10:22:33+09:00");
        assert!(issue_updated(&json!({})).is_none());
    }

    #[cfg(not(feature = "atlassian-http"))]
    #[tokio::test]
    async fn skeleton_is_safe_noop() {
        let conn = JiraConnector::new(Arc::new(FakeCreds(None)));
        let (items, c) = conn
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert!(items.is_empty());
        assert_eq!(c, Cursor::default());
    }
}
