//! Credential field specs for source connectors.
//!
//! Mirrors the alpha-agent-v3 "declare the field names, values live in the
//! central vault" pattern: a connector declares *which* fields it needs (name,
//! label, whether it is a secret, whether it is required) and the UI renders a
//! form from that. The values themselves are stored encrypted via
//! [`crate::core::traits::CredentialStore`] — never in code.
//!
//! The spec is the single source of truth the frontend reads (through the
//! `list_sources` command), so adding a new source means adding a connector +
//! its `FieldSpec`s here; no UI or command changes.

use serde::{Deserialize, Serialize};

use crate::core::types::{Credential, SourceKind};

/// One credential field a source requires the owner to fill in.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldSpec {
    /// The JSON key the value is stored under inside the [`Credential`].
    pub key: String,
    /// Human-facing label shown next to the input.
    pub label: String,
    /// Short hint / placeholder (e.g. "https://…").
    pub placeholder: String,
    /// A secret (token / password) → rendered as a password input and never
    /// echoed back to the frontend once stored.
    pub secret: bool,
    /// Whether the source is unusable until this field is filled.
    pub required: bool,
}

impl FieldSpec {
    fn secret(key: &str, label: &str, placeholder: &str) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            placeholder: placeholder.into(),
            secret: true,
            required: true,
        }
    }

    fn text(key: &str, label: &str, placeholder: &str) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            placeholder: placeholder.into(),
            secret: false,
            required: true,
        }
    }

    fn optional_text(key: &str, label: &str, placeholder: &str) -> Self {
        Self {
            required: false,
            ..Self::text(key, label, placeholder)
        }
    }
}

/// The two Atlassian connectors share one credential shape (Server/DC PAT).
/// Confluence adds an optional web address because the in-house API host is a
/// read-only mirror while the links people click live on the real server.
fn atlassian_fields(product: &str, example_host: &str, web_link: bool) -> Vec<FieldSpec> {
    use super::atlassian::{KEY_BASE_URL, KEY_PAT, KEY_WEB_BASE_URL};
    let mut fields = vec![
        FieldSpec::text(
            KEY_BASE_URL,
            &format!("{product} 서버 주소"),
            &format!("https://{example_host}  (API가 열려 있는 주소, 끝에 / 없이)"),
        ),
        FieldSpec::secret(
            KEY_PAT,
            "개인 액세스 토큰 (PAT)",
            &format!("{product} 프로필 → Personal Access Tokens 에서 발급"),
        ),
    ];
    if web_link {
        fields.push(FieldSpec::optional_text(
            KEY_WEB_BASE_URL,
            "링크용 주소 (선택)",
            "API 주소가 mirror 서버라면 사람이 여는 원본 서버 주소",
        ));
    }
    fields
}

/// The credential fields a source declares. Empty = no credentials needed
/// (Session/File work the moment the vault opens).
pub fn credential_spec(source: SourceKind) -> Vec<FieldSpec> {
    match source {
        SourceKind::Session | SourceKind::File => Vec::new(),
        SourceKind::Notion => vec![FieldSpec::secret(
            "token",
            "Integration 토큰",
            "secret_xxx (Notion integration의 Internal Integration Token)",
        )],
        SourceKind::Gmail => vec![
            FieldSpec::text("address", "Gmail 주소", "you@gmail.com"),
            FieldSpec::secret(
                "app_password",
                "앱 비밀번호",
                "Google 계정 → 보안 → 앱 비밀번호에서 발급",
            ),
        ],
        SourceKind::Confluence => atlassian_fields("Confluence", "confluence.example.com", true),
        SourceKind::Jira => atlassian_fields("Jira", "jira.example.com", false),
    }
}

/// Whether a stored credential satisfies every required field in the spec.
/// A field counts as present when its JSON value is a non-empty string.
pub fn credential_satisfies(source: SourceKind, cred: Option<&Credential>) -> bool {
    let spec = credential_spec(source);
    let required: Vec<&FieldSpec> = spec.iter().filter(|f| f.required).collect();
    if required.is_empty() {
        // No credentials required → always ready (Session/File).
        return true;
    }
    let Some(cred) = cred else {
        return false;
    };
    required.iter().all(|f| {
        cred.0
            .get(&f.key)
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty())
    })
}

/// Validate a source's credential values at connect time, returning a
/// human-readable note about what the connection can see.
///
/// - Always checks required fields are present (so an empty submit is rejected
///   with a clear message).
/// - When the connector can reach its service (feature-gated), performs a live
///   handshake so a bad token is caught immediately instead of on first sync,
///   and reports reachable scope (e.g. Notion's shared-page count) so the owner
///   learns *now* that a valid token with 0 shared pages will collect nothing.
///   Without the feature this step is skipped — the crate still builds/tests
///   offline and the connect just persists the credential.
///
/// A returned `Ok(msg)` means "safe to save"; `msg` may be a success note or a
/// caveat (valid token but nothing shared). Only a hard failure (bad token,
/// missing field) returns `Err`.
pub async fn verify_credentials(
    source: SourceKind,
    values: &serde_json::Value,
) -> crate::core::error::Result<String> {
    // 1. Required-field presence (works in every build).
    let spec = credential_spec(source);
    let missing: Vec<String> = spec
        .iter()
        .filter(|f| f.required)
        .filter(|f| {
            !values
                .get(&f.key)
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.trim().is_empty())
        })
        .map(|f| f.label.clone())
        .collect();
    if !missing.is_empty() {
        return Err(crate::core::error::AppError::InvalidInput(format!(
            "필수 항목을 입력하세요: {}",
            missing.join(", ")
        )));
    }

    // 2. Shape checks that need no network: an Atlassian PAT copied through
    //    an IME, or a base URL that is a bare host, fails later with a message
    //    that hides the cause — reject them here with one that says it.
    if matches!(source, SourceKind::Confluence | SourceKind::Jira) {
        use super::atlassian::{validate_base_url, validate_pat, KEY_BASE_URL, KEY_PAT};
        let field = |k: &str| values.get(k).and_then(|v| v.as_str()).unwrap_or_default();
        validate_base_url(field(KEY_BASE_URL))?;
        validate_pat(field(KEY_PAT).trim())?;
    }

    // 3. Live handshake where supported.
    match source {
        #[cfg(feature = "notion-http")]
        SourceKind::Notion => {
            let token = values
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let reachable = super::notion::http::verify(token).await?;
            return Ok(if reachable == 0 {
                "토큰은 유효하지만 연결된 페이지가 없습니다. Notion에서 최상위 \
                 페이지의 ⋯ → 연결 → 이 integration을 추가하면 하위 페이지까지 \
                 수집됩니다."
                    .to_string()
            } else {
                format!("연결됨 · 접근 가능한 페이지 {reachable}개 이상")
            });
        }
        #[cfg(feature = "atlassian-http")]
        SourceKind::Confluence => {
            let creds =
                super::atlassian::creds_from(Some(&Credential(values.clone())), "confluence")?;
            let (who, count) = super::confluence::http::verify(&creds).await?;
            return Ok(match count {
                Some(0) => format!(
                    "연결됨 · {who} · 내가 작성·수정한 페이지가 검색되지 않습니다. 이 계정으로 쓴 페이지가 있는 서버인지 확인하세요."
                ),
                Some(n) => format!("연결됨 · {who} · 내가 작성·수정한 페이지 {n}개"),
                None => format!("연결됨 · {who}"),
            });
        }
        #[cfg(feature = "atlassian-http")]
        SourceKind::Jira => {
            let creds = super::atlassian::creds_from(Some(&Credential(values.clone())), "jira")?;
            let (who, count) = super::jira::http::verify(&creds).await?;
            return Ok(match count {
                Some(0) => format!(
                    "연결됨 · {who} · 내가 담당·보고한 이슈가 검색되지 않습니다. 이 계정으로 일한 Jira인지 확인하세요."
                ),
                Some(n) => format!("연결됨 · {who} · 내가 담당·보고한 이슈 {n}개"),
                None => format!("연결됨 · {who}"),
            });
        }
        _ => {}
    }
    Ok("연결되었습니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credential_less_sources_are_always_ready() {
        assert!(credential_spec(SourceKind::Session).is_empty());
        assert!(credential_satisfies(SourceKind::Session, None));
        assert!(credential_satisfies(SourceKind::File, None));
    }

    #[test]
    fn notion_requires_a_token() {
        assert!(!credential_satisfies(SourceKind::Notion, None));
        let empty = Credential(json!({ "token": "" }));
        assert!(!credential_satisfies(SourceKind::Notion, Some(&empty)));
        let filled = Credential(json!({ "token": "secret_abc" }));
        assert!(credential_satisfies(SourceKind::Notion, Some(&filled)));
    }

    #[test]
    fn atlassian_sources_need_base_url_and_pat() {
        for kind in [SourceKind::Confluence, SourceKind::Jira] {
            assert!(!credential_satisfies(kind, None));
            let no_pat = Credential(json!({ "base_url": "https://x.example.com" }));
            assert!(!credential_satisfies(kind, Some(&no_pat)));
            let full = Credential(json!({ "base_url": "https://x.example.com", "pat": "tok" }));
            assert!(credential_satisfies(kind, Some(&full)));
        }
        // The optional link address is not required and Jira does not have it.
        let conf = credential_spec(SourceKind::Confluence);
        assert_eq!(conf.iter().filter(|f| f.required).count(), 2);
        assert!(conf.iter().any(|f| f.key == "web_base_url" && !f.required));
        assert!(credential_spec(SourceKind::Jira)
            .iter()
            .all(|f| f.key != "web_base_url"));
        assert!(conf.iter().find(|f| f.key == "pat").unwrap().secret);
    }

    #[tokio::test]
    async fn atlassian_shape_checks_run_before_any_network() {
        // A bare host and an IME-polluted PAT are rejected in every build,
        // with the cause in the message.
        let bare = json!({ "base_url": "jira.example.com", "pat": "tok" });
        let err = verify_credentials(SourceKind::Jira, &bare)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("http://"));
        let polluted = json!({ "base_url": "https://jira.example.com", "pat": "tokㅇ" });
        let err = verify_credentials(SourceKind::Jira, &polluted)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("이상 문자"));
        let missing = json!({ "base_url": "https://jira.example.com" });
        let err = verify_credentials(SourceKind::Confluence, &missing)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("필수 항목"));
    }

    #[test]
    fn gmail_requires_all_fields() {
        let partial = Credential(json!({ "address": "me@gmail.com" }));
        assert!(!credential_satisfies(SourceKind::Gmail, Some(&partial)));
        let whitespace = Credential(json!({ "address": "me@gmail.com", "app_password": "   " }));
        assert!(!credential_satisfies(SourceKind::Gmail, Some(&whitespace)));
        let full = Credential(json!({ "address": "me@gmail.com", "app_password": "pw" }));
        assert!(credential_satisfies(SourceKind::Gmail, Some(&full)));
    }
}
