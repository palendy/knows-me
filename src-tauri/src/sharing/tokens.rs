//! Consumer token minting, validation, and revocation (step ⓑ).
//!
//! This is the persistence behind the auth seam in [`super::mcp::resolve_identity`]:
//! it turns a Bearer secret into a scoped [`Token::consumer`] (`mcp-contract.md`
//! §6), or refuses it. Authorization itself is unchanged — once a consumer token
//! is built, `Token::can_access` already narrows the view to
//! `granted ∩ {Shared}`. All this module adds is *who the caller is*.
//!
//! Design (see `jb-docs/space-a-notes.md` for the anti-patterns being avoided):
//! - **High-entropy random secrets.** 256 bits from the OS CSPRNG, never a
//!   sequential/guessable id.
//! - **The raw secret never touches disk in the clear.** Records are keyed by
//!   `base64url(SHA-256(secret))`, so the filename is a digest, not the token;
//!   the value is then AES-GCM-encrypted by [`EncryptedStore`] like everything
//!   else. Lookup stays O(1) — hash the presented secret, read one key.
//! - **Revocation is immediate.** A revoked record resolves to `None` (→ 401) on
//!   the very next request; there is no grace window.
//! - **Header-only.** The secret rides `Authorization: Bearer` (`mcp-contract.md`
//!   §10.3) — this module never puts it in a URL.

use std::collections::BTreeSet;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::Category;
use crate::sharing::Token;

/// Storage namespace for consumer token records. Deliberately distinct from U3's
/// knowledge layout so the two never collide.
const NS: &str = "sharing-tokens";
/// Secret length in bytes (256 bits of CSPRNG entropy).
const SECRET_BYTES: usize = 32;

/// The persisted form of a consumer grant. Encrypted at rest by
/// [`EncryptedStore`]; the storage *key* is a hash of the secret (never the
/// secret itself), so this record is only ever reached by presenting the token.
#[derive(Serialize, Deserialize)]
struct TokenRecord {
    /// Human-facing consumer label (e.g. `"alice"`), for the owner's UI/audit and
    /// as the [`Token::id`]. Not a secret; not used for lookup.
    id: String,
    /// Categories this token may read. Combined with `visibility == Shared` by
    /// [`Token::can_access`] — this set alone never widens the owner's exposure.
    granted: BTreeSet<Category>,
    /// When the token was minted. Surfaced to the owner's audit/UI via
    /// [`TokenStore::list`] and kept as the basis for a future TTL; tokens are
    /// otherwise valid until explicitly revoked (the deliberate model — a leaked
    /// secret is handled by [revoking], not by expiry).
    ///
    /// [revoking]: TokenStore::revoke
    issued_at: DateTime<Utc>,
    /// Once true, [`TokenStore::resolve`] refuses the token immediately.
    revoked: bool,
}

/// A freshly minted token. The `secret` is visible **only here, once** — it is
/// never recoverable from storage (only its hash is stored). The owner shows it
/// to the consumer out-of-band; losing it means reissuing.
pub struct IssuedToken {
    pub id: String,
    pub secret: String,
}

/// A live token's public metadata, for the owner's sharing UI. Never carries the
/// secret (only its hash is stored, and even that stays inside the store).
pub struct TokenInfo {
    pub id: String,
    /// The granted categories, in stable sorted order.
    pub granted: Vec<Category>,
    pub issued_at: DateTime<Utc>,
}

/// Mints, validates, and revokes consumer tokens on top of an [`EncryptedStore`].
pub struct TokenStore {
    store: Arc<dyn EncryptedStore>,
}

impl TokenStore {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Mint a token for `id` granting `granted`. Returns the one-time secret.
    pub async fn issue(
        &self,
        id: impl Into<String>,
        granted: impl IntoIterator<Item = Category>,
    ) -> Result<IssuedToken> {
        let id = id.into();
        let secret = generate_secret();
        let record = TokenRecord {
            id: id.clone(),
            granted: granted.into_iter().collect(),
            issued_at: Utc::now(),
            revoked: false,
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| AppError::Serde(e.to_string()))?;
        self.store.put(NS, &key_for(&secret), &bytes).await?;
        Ok(IssuedToken { id, secret })
    }

    /// Resolve a presented Bearer secret to a scoped [`Token::consumer`], or
    /// `None` when it is unknown, malformed, or revoked (all indistinguishable to
    /// the caller — §5.1 `unauthorized`). A [`AppError::Locked`]/store error
    /// propagates so the transport can report it distinctly from a bad token.
    pub async fn resolve(&self, secret: &str) -> Result<Option<Token>> {
        let Some(bytes) = self.store.get(NS, &key_for(secret)).await? else {
            return Ok(None);
        };
        let record: TokenRecord =
            serde_json::from_slice(&bytes).map_err(|e| AppError::Serde(e.to_string()))?;
        if record.revoked {
            return Ok(None);
        }
        Ok(Some(Token::consumer(record.id, record.granted)))
    }

    /// Revoke every live token issued under `id`; returns whether any changed.
    ///
    /// The owner revokes by the label they issued under (they never keep the
    /// secret), so this scans the namespace. Revocation is a rare admin action at
    /// personal scale, so the O(n) scan is acceptable; the hot path ([`resolve`])
    /// stays O(1).
    ///
    /// [`resolve`]: Self::resolve
    pub async fn revoke(&self, id: &str) -> Result<bool> {
        let keys = self.store.list(NS).await?;
        let mut revoked_any = false;
        for key in keys {
            // A record we cannot read or parse cannot authenticate either
            // ([`resolve`] fails it the same way), so it can never be a live token
            // for `id`; skip it rather than let one corrupt or foreign entry abort
            // the scan and leave `id`'s *other* tokens live — a silent revocation
            // failure is the last thing a security operation should do. A locked
            // vault is different: we cannot revoke anything, so surface it.
            //
            // [`resolve`]: Self::resolve
            let mut record: TokenRecord = match self.store.get(NS, &key).await {
                Ok(Some(bytes)) => match serde_json::from_slice(&bytes) {
                    Ok(r) => r,
                    Err(_) => continue,
                },
                Ok(None) => continue,
                Err(AppError::Locked) => return Err(AppError::Locked),
                Err(_) => continue,
            };
            if record.id == id && !record.revoked {
                record.revoked = true;
                let bytes =
                    serde_json::to_vec(&record).map_err(|e| AppError::Serde(e.to_string()))?;
                // A genuine write failure on a token we *are* revoking is surfaced
                // (not swallowed); a retry skips the ones already flipped.
                self.store.put(NS, &key, &bytes).await?;
                revoked_any = true;
            }
        }
        Ok(revoked_any)
    }

    /// List every live (non-revoked) token's public metadata, newest first, for
    /// the owner's sharing UI. The secret is never stored, so it is never
    /// returned. Like [`revoke`], a record that cannot be read or parsed is
    /// skipped (it cannot authenticate either, so it is not a live token); a
    /// locked vault propagates.
    ///
    /// [`revoke`]: Self::revoke
    pub async fn list(&self) -> Result<Vec<TokenInfo>> {
        let keys = self.store.list(NS).await?;
        let mut out = Vec::new();
        for key in keys {
            let record: TokenRecord = match self.store.get(NS, &key).await {
                Ok(Some(bytes)) => match serde_json::from_slice(&bytes) {
                    Ok(r) => r,
                    Err(_) => continue,
                },
                Ok(None) => continue,
                Err(AppError::Locked) => return Err(AppError::Locked),
                Err(_) => continue,
            };
            if record.revoked {
                continue;
            }
            out.push(TokenInfo {
                id: record.id,
                // BTreeSet iterates sorted; collect into the stable-ordered Vec.
                granted: record.granted.into_iter().collect(),
                issued_at: record.issued_at,
            });
        }
        // Newest first, then id, so the UI order is stable across calls.
        out.sort_by(|a, b| b.issued_at.cmp(&a.issued_at).then_with(|| a.id.cmp(&b.id)));
        Ok(out)
    }
}

/// A fresh 256-bit secret, base64url-encoded for use as a Bearer token. The raw
/// bytes are wiped after encoding.
fn generate_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let secret = B64URL.encode(bytes);
    bytes.zeroize();
    secret
}

/// The storage key for a secret: `base64url(SHA-256(secret))`. A digest, so the
/// raw secret never appears in a filename; deterministic, so lookup is a single
/// keyed read.
fn key_for(secret: &str) -> String {
    B64URL.encode(Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryStore;

    fn cat(s: &str) -> Category {
        Category::parse(s).unwrap()
    }

    fn store() -> TokenStore {
        TokenStore::new(Arc::new(InMemoryStore::default()))
    }

    #[tokio::test]
    async fn issue_then_resolve_yields_a_scoped_consumer() {
        let ts = store();
        let issued = ts.issue("alice", [cat("deploy")]).await.unwrap();
        let token = ts.resolve(&issued.secret).await.unwrap().unwrap();
        assert!(!token.owner);
        assert_eq!(token.id, "alice");
        // The grant is carried by the token and drives `can_access`.
        assert!(token.can_access(crate::core::types::Visibility::Shared, Some(&cat("deploy"))));
        assert!(!token.can_access(
            crate::core::types::Visibility::Shared,
            Some(&cat("payment"))
        ));
        assert!(!token.can_access(
            crate::core::types::Visibility::Private,
            Some(&cat("deploy"))
        ));
    }

    #[tokio::test]
    async fn unknown_or_malformed_secret_resolves_to_none() {
        let ts = store();
        ts.issue("alice", [cat("deploy")]).await.unwrap();
        assert!(ts.resolve("not-a-real-token").await.unwrap().is_none());
        assert!(ts.resolve("").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn revoke_is_immediate() {
        let ts = store();
        let issued = ts.issue("alice", [cat("deploy")]).await.unwrap();
        assert!(ts.resolve(&issued.secret).await.unwrap().is_some());

        assert!(
            ts.revoke("alice").await.unwrap(),
            "a live token was revoked"
        );
        // Next request already fails — no grace window.
        assert!(ts.resolve(&issued.secret).await.unwrap().is_none());
        // Revoking again is idempotent and reports nothing changed.
        assert!(!ts.revoke("alice").await.unwrap());
    }

    #[tokio::test]
    async fn revoke_targets_one_consumer_and_leaves_others() {
        let ts = store();
        let alice = ts.issue("alice", [cat("deploy")]).await.unwrap();
        let bob = ts.issue("bob", [cat("payment")]).await.unwrap();
        ts.revoke("alice").await.unwrap();
        assert!(ts.resolve(&alice.secret).await.unwrap().is_none());
        assert!(ts.resolve(&bob.secret).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn revoke_skips_corrupt_records_and_still_revokes_the_target() {
        // A corrupt or foreign entry in the namespace must not abort revocation of
        // a live token — a silent revocation failure would leave access open.
        let backing = Arc::new(InMemoryStore::default());
        let ts = TokenStore::new(backing.clone());
        let issued = ts.issue("alice", [cat("deploy")]).await.unwrap();
        backing
            .put(NS, "corrupt-key", b"not valid json")
            .await
            .unwrap();

        assert!(
            ts.revoke("alice").await.unwrap(),
            "alice is revoked despite the corrupt neighbor record"
        );
        assert!(ts.resolve(&issued.secret).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_returns_live_tokens_newest_first_without_revoked() {
        let ts = store();
        ts.issue("alice", [cat("deploy")]).await.unwrap();
        ts.issue("bob", [cat("payment"), cat("deploy")])
            .await
            .unwrap();
        ts.revoke("bob").await.unwrap();

        let listed = ts.list().await.unwrap();
        assert_eq!(listed.len(), 1, "the revoked token must be excluded");
        assert_eq!(listed[0].id, "alice");
        assert_eq!(
            listed[0]
                .granted
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>(),
            vec!["deploy"],
            "granted categories are surfaced (never the secret)"
        );
    }

    #[tokio::test]
    async fn list_skips_corrupt_records() {
        let backing = Arc::new(InMemoryStore::default());
        let ts = TokenStore::new(backing.clone());
        ts.issue("alice", [cat("deploy")]).await.unwrap();
        backing.put(NS, "corrupt-key", b"not json").await.unwrap();

        let listed = ts.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "alice");
    }

    #[tokio::test]
    async fn secrets_are_high_entropy_and_distinct() {
        let ts = store();
        let a = ts.issue("alice", [cat("deploy")]).await.unwrap();
        let b = ts.issue("alice", [cat("deploy")]).await.unwrap();
        assert_ne!(a.secret, b.secret, "two mints must not collide");
        // 256 bits base64url-encodes to 43 chars; guard against a truncated CSPRNG.
        assert!(a.secret.len() >= 43, "secret too short: {}", a.secret.len());
    }

    #[tokio::test]
    async fn raw_secret_never_appears_in_a_storage_key() {
        // The on-disk key is a digest; presenting the secret is the only way back
        // to the record. A leaked filesystem listing must not reveal tokens.
        let backing = Arc::new(InMemoryStore::default());
        let ts = TokenStore::new(backing.clone());
        let issued = ts.issue("alice", [cat("deploy")]).await.unwrap();
        let keys = backing.list(NS).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert!(
            !keys[0].contains(&issued.secret),
            "secret leaked into the key"
        );
        assert_eq!(keys[0], key_for(&issued.secret));
    }
}
