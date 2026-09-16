//! Shared plumbing for the Atlassian **Server / Data Center** connectors
//! ([`super::confluence`], [`super::jira`]).
//!
//! Ported from alpha-agent-v3's `skills/{confluence,jira}/scripts/_common.py`,
//! which is what actually runs against the in-house instances. The rules that
//! matter there are kept here, because every one of them was learned the hard
//! way:
//!
//!  - Auth is a **Personal Access Token** sent as `Authorization: Bearer` —
//!    not Cloud's email + API-token basic auth. Cloud is not a target.
//!  - A PAT pasted through a Korean IME can carry non-ASCII / whitespace and
//!    then dies inside header encoding with an unreadable error → reject it
//!    up front with a message that says what to fix.
//!  - **401 and 403 are different answers.** 401 = the PAT is bad. 403 = the
//!    PAT is fine but the document/issue is restricted (classification, space
//!    or project permission). Telling the owner to re-enter the PAT on a 403
//!    is an infinite loop. The split is decided by response headers
//!    (`X-AUSERNAME` = authenticated, `X-Authentication-Denied-Reason` =
//!    the server refused authentication itself), not by keyword guessing.
//!  - A server behind SSO can answer an expired PAT with **200 + an HTML
//!    login page**. Non-JSON on success is therefore an auth failure.
//!  - The Confluence mirror rate-limits (~15 calls/min per account) and
//!    answers 429 with `Retry-After`; 5xx happens. Both are retried briefly.
//!
//! Everything outside the `http` submodule is pure and builds without the
//! `atlassian-http` feature, so cursor math, credential parsing and text
//! rendering are unit-tested in the default (offline) `cargo test`.

use chrono::{DateTime, Duration, FixedOffset};

use crate::core::error::{AppError, Result};
use crate::core::types::Credential;

/// Character cap per collected item (page / issue), head kept. Same budget as
/// the session connector: past this a single model call cannot take it, and
/// for a document the overview lives at the top.
pub const MAX_CHARS_PER_ITEM: usize = 24_000;

/// Credential field keys shared by both connectors (`FieldSpec.key`).
pub const KEY_BASE_URL: &str = "base_url";
pub const KEY_PAT: &str = "pat";
/// Confluence only: the original (non-mirror) server used for links the owner
/// clicks. API calls always go to `base_url`.
pub const KEY_WEB_BASE_URL: &str = "web_base_url";

/// What a connector needs to talk to one Atlassian server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlassianCreds {
    /// Normalized: no trailing slash.
    pub base_url: String,
    pub pat: String,
    /// Normalized like `base_url`; `None` when not set.
    pub web_base_url: Option<String>,
}

impl AtlassianCreds {
    /// The base to build user-facing links from (web base if given).
    pub fn link_base(&self) -> &str {
        self.web_base_url.as_deref().unwrap_or(&self.base_url)
    }
}

/// Trim and drop trailing slashes so `{base}/rest/api/...` joins cleanly.
pub fn normalize_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// A base URL must be an absolute http(s) URL — anything else (a bare host,
/// a `/wiki` path) fails later with a confusing connection error.
pub fn validate_base_url(url: &str) -> Result<()> {
    let u = url.trim();
    if u.starts_with("http://") || u.starts_with("https://") {
        let rest = u.split("://").nth(1).unwrap_or_default();
        if !rest.is_empty() && !rest.starts_with('/') {
            return Ok(());
        }
    }
    Err(AppError::InvalidInput(format!(
        "서버 주소는 http:// 또는 https:// 로 시작하는 전체 주소여야 합니다: {u:?}"
    )))
}

/// Reject a PAT that cannot be put in an HTTP header. alpha-agent-v3 #67: a
/// token copied through an IME picked up a Korean character and every call
/// died with `UnicodeEncodeError` — the owner saw a traceback, not the cause.
pub fn validate_pat(pat: &str) -> Result<()> {
    if pat.is_empty() {
        return Err(AppError::InvalidInput("PAT를 입력하세요".into()));
    }
    if !pat.is_ascii() || pat.chars().any(char::is_whitespace) {
        return Err(AppError::InvalidInput(
            "PAT 값에 이상 문자(한글/공백 등)가 섞여 있습니다 — 토큰을 다시 복사해 붙여넣으세요"
                .into(),
        ));
    }
    Ok(())
}

/// Read `{base_url, pat, web_base_url?}` out of a stored credential. A missing
/// or empty required value → `External("reauth required: {tag}")`, the shape
/// the UI turns into a (re)connect prompt.
pub fn creds_from(cred: Option<&Credential>, tag: &str) -> Result<AtlassianCreds> {
    let get = |k: &str| -> Option<String> {
        cred.and_then(|c| c.0.get(k))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match (get(KEY_BASE_URL), get(KEY_PAT)) {
        (Some(base), Some(pat)) => Ok(AtlassianCreds {
            base_url: normalize_base_url(&base),
            pat,
            web_base_url: get(KEY_WEB_BASE_URL).map(|u| normalize_base_url(&u)),
        }),
        _ => Err(AppError::External(format!("reauth required: {tag}"))),
    }
}

// ---------------------------------------------------------------------------
// Time cursors
// ---------------------------------------------------------------------------

/// Parse the timestamp shapes the two servers emit:
/// Confluence `version.when` = `2026-09-15T10:22:33.000+09:00` (RFC 3339),
/// Jira `fields.updated`     = `2026-09-15T10:22:33.000+0900`.
///
/// The offset is kept (not converted to UTC) because the query window has to
/// be phrased in the server's own local time — see [`window_start`].
pub fn parse_atlassian_time(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f%z").ok())
        .or_else(|| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z").ok())
}

/// The `>=` bound to put in CQL/JQL when continuing from `cursor`.
///
/// CQL `lastModified` / JQL `updated` compare against a *local* time in the
/// requesting user's Jira/Confluence profile timezone, which the app cannot
/// see. The timestamps the servers return do carry an offset, and it is the
/// same profile timezone, so the bound is formatted in the cursor's own offset
/// — and pushed back a day so a DST edge or a profile-timezone mismatch can
/// only cause a re-scan, never a gap. Items at or before the cursor are then
/// skipped client-side (the seen-set is the real dedup gate anyway).
pub fn window_start(cursor: &DateTime<FixedOffset>) -> String {
    (*cursor - Duration::days(1))
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

// ---------------------------------------------------------------------------
// Text shaping
// ---------------------------------------------------------------------------

/// Rendered Confluence HTML (`body.view`) → plain text.
///
/// Deliberately regex-only (alpha-agent-v3 does the same): block-level closes
/// become newlines, table cells become ` | `, the rest of the markup is
/// dropped, the handful of entities Confluence actually emits are decoded.
/// Good enough for a model to read; a real HTML parser is not worth a dep.
pub fn html_to_text(html: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    static DROP_BLOCKS: OnceLock<Regex> = OnceLock::new();
    static NEWLINE_TAGS: OnceLock<Regex> = OnceLock::new();
    static CELL_TAGS: OnceLock<Regex> = OnceLock::new();
    static ANY_TAG: OnceLock<Regex> = OnceLock::new();
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    static SPACES: OnceLock<Regex> = OnceLock::new();
    static BLANK_LINES: OnceLock<Regex> = OnceLock::new();

    let drop = DROP_BLOCKS
        .get_or_init(|| Regex::new(r"(?is)<(script|style)\b.*?</(script|style)>").unwrap());
    let nl = NEWLINE_TAGS.get_or_init(|| {
        Regex::new(
            r"(?i)</(p|li|h[1-6]|div|tr|table|ul|ol|blockquote|pre|section|article|header|footer)>|<br\s*/?>|<hr\s*/?>",
        )
        .unwrap()
    });
    let cell = CELL_TAGS.get_or_init(|| Regex::new(r"(?i)</(td|th)>").unwrap());
    let any = ANY_TAG.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap());
    let entity =
        ENTITY.get_or_init(|| Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);").unwrap());
    let spaces = SPACES.get_or_init(|| Regex::new(r"[ \t\u{a0}]*\n[ \t\u{a0}]*").unwrap());
    let blanks = BLANK_LINES.get_or_init(|| Regex::new(r"\n{3,}").unwrap());

    let s = drop.replace_all(html, "");
    let s = nl.replace_all(&s, "\n");
    let s = cell.replace_all(&s, " | ");
    let s = any.replace_all(&s, "");
    let s = entity.replace_all(&s, |c: &regex::Captures| {
        let e = &c[1];
        match e {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            "nbsp" => " ".to_string(),
            _ => {
                let code = if let Some(hex) = e.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = e.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32)
                    .map(|ch| ch.to_string())
                    .unwrap_or_else(|| c[0].to_string())
            }
        }
    });
    let s = spaces.replace_all(&s, "\n");
    let s = blanks.replace_all(&s, "\n\n");
    s.trim().to_string()
}

/// Keep the first `max` characters (documents front-load their overview).
pub fn head_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str("\n…(이하 생략)");
    out
}

/// A `Value` string field, or "" — the servers omit optional fields rather
/// than nulling them, so every read has to tolerate absence.
pub fn str_at<'a>(v: &'a serde_json::Value, path: &[&str]) -> &'a str {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_str().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// HTTP (feature `atlassian-http`)
// ---------------------------------------------------------------------------

/// Real HTTP against an Atlassian server: Bearer client, brief retries, and
/// the 401 / 403 / HTML-login classification described in the module docs.
#[cfg(feature = "atlassian-http")]
pub(crate) mod http {
    use std::time::Duration;

    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
    use reqwest::{RequestBuilder, StatusCode};
    use serde_json::Value;

    use crate::core::error::{AppError, Result};

    /// One request's wall-clock budget. The Confluence mirror can be slow on
    /// big pages; 15 s (alpha-agent-v3's value) was tuned to a 30 s exec cap
    /// that does not apply here.
    const TIMEOUT: Duration = Duration::from_secs(45);

    pub fn client(pat: &str) -> Result<reqwest::Client> {
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&format!("Bearer {pat}"))
            .map_err(|e| AppError::InvalidInput(format!("PAT를 헤더에 넣을 수 없습니다: {e}")))?;
        auth.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        // Proxy comes from HTTP(S)_PROXY / NO_PROXY (reqwest default) and TLS
        // roots from the OS store (+ SSL_CERT_FILE) via rustls-native-roots —
        // that is how an in-house server behind a corporate CA verifies.
        reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("knows-me/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| AppError::External(format!("HTTP 클라이언트 생성 실패: {e}")))
    }

    /// Send, retrying the transient cases, then hand back status + headers +
    /// body text (the body is read fully so a non-JSON answer can be shown).
    ///
    /// Retries: 429 up to 3× honoring `Retry-After` (capped at 60 s), 5xx
    /// once, connection errors twice with a short backoff. Timeouts are not
    /// retried — they rarely resolve on a retry and just double the wait.
    async fn send_with_retry<F>(
        make: F,
        service: &str,
        what: &str,
    ) -> Result<(StatusCode, HeaderMap, String)>
    where
        F: Fn() -> RequestBuilder,
    {
        let mut attempt: u32 = 0;
        loop {
            match make().send().await {
                Err(e) => {
                    if e.is_connect() && attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                        attempt += 1;
                        continue;
                    }
                    let why = if e.is_timeout() {
                        "응답 시간 초과"
                    } else if e.is_connect() {
                        "서버에 연결할 수 없음 (주소·프록시·NO_PROXY 확인)"
                    } else {
                        "요청 실패"
                    };
                    return Err(AppError::External(format!("{service} {what}: {why} — {e}")));
                }
                Ok(resp) => {
                    let status = resp.status();
                    if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                        let wait = resp
                            .headers()
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .map(|s| s.min(60))
                            .unwrap_or(2u64.pow(attempt));
                        eprintln!("[{service}] 429 on {what}; waiting {wait}s");
                        tokio::time::sleep(Duration::from_secs(wait)).await;
                        attempt += 1;
                        continue;
                    }
                    if status.is_server_error() && attempt < 1 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        attempt += 1;
                        continue;
                    }
                    let headers = resp.headers().clone();
                    let text = resp.text().await.map_err(|e| {
                        AppError::External(format!("{service} {what}: 본문 읽기 실패 — {e}"))
                    })?;
                    return Ok((status, headers, text));
                }
            }
        }
    }

    /// Turn a non-2xx answer into the message the owner should actually see.
    fn classify(
        status: StatusCode,
        headers: &HeaderMap,
        body: &str,
        service: &str,
        what: &str,
    ) -> AppError {
        let hdr = |k: &str| {
            headers
                .get(k)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        match status.as_u16() {
            401 => AppError::External(format!(
                "{service} 인증에 실패했습니다 (401) — PAT가 만료됐거나 잘못됐습니다. 토큰을 다시 발급해 연결하세요."
            )),
            403 => {
                if let Some(reason) = hdr("X-Authentication-Denied-Reason") {
                    // The server refused authentication itself (CAPTCHA lock etc.).
                    AppError::External(format!(
                        "{service} 인증이 거부됐습니다 ({reason}). 브라우저로 {service}에 한 번 로그인해 잠금을 풀고, 그래도 안 되면 PAT를 다시 발급하세요."
                    ))
                } else {
                    let who = hdr("X-AUSERNAME")
                        .filter(|u| !u.eq_ignore_ascii_case("anonymous"))
                        .map(|u| format!(" (서버 인증 계정: {u})"))
                        .unwrap_or_default();
                    AppError::External(format!(
                        "{service} 권한이 없습니다 (403){who}. 인증은 통과했으므로 PAT 문제가 아니라 문서/프로젝트 권한 설정입니다 — PAT를 다시 넣어도 해결되지 않습니다."
                    ))
                }
            }
            404 => AppError::External(format!(
                "{service} {what}: 없음 (404) — 서버 주소가 REST API를 노출하는 주소인지 확인하세요 (예: https://host, /wiki 나 /display 없이)."
            )),
            _ => {
                let snippet: String = body.chars().take(200).collect();
                AppError::External(format!("{service} {what}: HTTP {status} — {snippet}"))
            }
        }
    }

    /// Parse a 2xx body as JSON; a login page (HTML) on a 200 means the PAT
    /// was not accepted (SSO redirect), not that the server is broken.
    fn parse_json(
        status: StatusCode,
        headers: &HeaderMap,
        body: &str,
        service: &str,
        what: &str,
    ) -> Result<Value> {
        if !status.is_success() {
            return Err(classify(status, headers, body, service, what));
        }
        serde_json::from_str::<Value>(body).map_err(|_| {
            AppError::External(format!(
                "{service} {what}: 서버가 JSON 대신 로그인/HTML 페이지를 돌려줬습니다 — PAT가 유효하지 않거나 주소가 API 서버가 아닙니다."
            ))
        })
    }

    /// `GET {url}?{query}` → JSON.
    pub async fn get_json(
        client: &reqwest::Client,
        url: &str,
        query: &[(&str, String)],
        service: &str,
        what: &str,
    ) -> Result<Value> {
        let (status, headers, body) =
            send_with_retry(|| client.get(url).query(query), service, what).await?;
        parse_json(status, &headers, &body, service, what)
    }

    /// `POST {url}` with a JSON body → JSON.
    pub async fn post_json(
        client: &reqwest::Client,
        url: &str,
        payload: &Value,
        service: &str,
        what: &str,
    ) -> Result<Value> {
        let (status, headers, body) =
            send_with_retry(|| client.post(url).json(payload), service, what).await?;
        parse_json(status, &headers, &body, service, what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base_url_is_normalized_and_validated() {
        assert_eq!(
            normalize_base_url(" https://conf.example.com/ "),
            "https://conf.example.com"
        );
        assert_eq!(
            normalize_base_url("https://conf.example.com//"),
            "https://conf.example.com"
        );
        assert!(validate_base_url("https://conf.example.com").is_ok());
        assert!(validate_base_url("http://10.0.0.5:8090").is_ok());
        assert!(validate_base_url("conf.example.com").is_err());
        assert!(validate_base_url("https://").is_err());
        assert!(validate_base_url("").is_err());
    }

    #[test]
    fn pat_rejects_ime_garbage() {
        assert!(validate_pat("NjE2MzY5NDc0NTU5OrZ").is_ok());
        assert!(validate_pat("").is_err());
        assert!(validate_pat("abc def").is_err());
        assert!(validate_pat("abc\n").is_err());
        assert!(validate_pat("abcㅇ").is_err());
    }

    #[test]
    fn creds_come_from_the_stored_credential() {
        let cred = Credential(json!({
            "base_url": "https://mirror.example.com/",
            "pat": "tok",
            "web_base_url": "https://conf.example.com/"
        }));
        let c = creds_from(Some(&cred), "confluence").unwrap();
        assert_eq!(c.base_url, "https://mirror.example.com");
        assert_eq!(c.link_base(), "https://conf.example.com");

        let no_web = Credential(json!({ "base_url": "https://j.example.com", "pat": "tok" }));
        let c = creds_from(Some(&no_web), "jira").unwrap();
        assert_eq!(c.web_base_url, None);
        assert_eq!(c.link_base(), "https://j.example.com");

        let err = creds_from(None, "jira").unwrap_err();
        assert!(err.to_string().contains("reauth required: jira"));
        let blank = Credential(json!({ "base_url": "https://j.example.com", "pat": "  " }));
        assert!(creds_from(Some(&blank), "jira").is_err());
    }

    #[test]
    fn both_server_time_shapes_parse_and_keep_their_offset() {
        let conf = parse_atlassian_time("2026-09-15T10:22:33.000+09:00").unwrap();
        let jira = parse_atlassian_time("2026-09-15T10:22:33.000+0900").unwrap();
        assert_eq!(conf, jira);
        assert_eq!(conf.offset().local_minus_utc(), 9 * 3600);
        assert!(parse_atlassian_time("yesterday").is_none());
    }

    #[test]
    fn window_start_is_a_day_back_in_local_time() {
        let t = parse_atlassian_time("2026-09-15T00:30:00.000+09:00").unwrap();
        // Local (KST) minus one day, formatted local — not converted to UTC.
        assert_eq!(window_start(&t), "2026-09-14 00:30");
    }

    #[test]
    fn html_becomes_readable_text() {
        let html = r#"<h1>Title</h1><p>One&nbsp;&amp; <b>two</b></p><ul><li>a</li><li>b</li></ul>
<table><tr><th>k</th><th>v</th></tr><tr><td>x</td><td>1 &lt; 2</td></tr></table><script>alert(1)</script><p>&#39;q&#x41;</p>"#;
        let text = html_to_text(html);
        assert_eq!(
            text,
            "Title\nOne & two\na\nb\n\nk | v |\nx | 1 < 2 |\n\n'qA"
        );
    }

    #[test]
    fn head_chars_caps_and_marks() {
        assert_eq!(head_chars("abc", 5), "abc");
        let capped = head_chars("가나다라마바", 3);
        assert!(capped.starts_with("가나다"));
        assert!(capped.ends_with("생략)"));
    }

    #[test]
    fn str_at_tolerates_missing_paths() {
        let v = json!({ "a": { "b": "c" } });
        assert_eq!(str_at(&v, &["a", "b"]), "c");
        assert_eq!(str_at(&v, &["a", "zzz"]), "");
        assert_eq!(str_at(&v, &["nope", "b"]), "");
    }
}
