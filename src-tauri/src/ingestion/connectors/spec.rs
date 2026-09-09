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

    // 2. Live handshake where supported.
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
    fn gmail_requires_all_fields() {
        let partial = Credential(json!({ "address": "me@gmail.com" }));
        assert!(!credential_satisfies(SourceKind::Gmail, Some(&partial)));
        let whitespace = Credential(json!({ "address": "me@gmail.com", "app_password": "   " }));
        assert!(!credential_satisfies(SourceKind::Gmail, Some(&whitespace)));
        let full = Credential(json!({ "address": "me@gmail.com", "app_password": "pw" }));
        assert!(credential_satisfies(SourceKind::Gmail, Some(&full)));
    }
}
