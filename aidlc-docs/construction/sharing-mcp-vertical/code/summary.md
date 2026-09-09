# Sharing / MCP Vertical — Code Generation Summary

> Markdown summary of the owner's MCP sharing vertical (the code itself lives under `src-tauri/src/sharing/`, `desktop/src/`, and `src/features/sharing/`).
> Canonical contract: [`docs/mcp-contract.md`](../../../../docs/mcp-contract.md). This vertical implements that contract (tools 4종, 인가 시맨틱, 에러, 서빙 안전, 두 모드).
>
> **Workflow note**: this vertical was built outside the heavy AI-DLC stage-gate ceremony (owner solo, contract-first). It is not one of the U1–U4 units of work; it is the cross-cutting sharing layer that sits on top of the frozen `sharing::` contract (PR #6). Recorded here for construction traceability.

## Scope

The MCP vertical, owned solo: **ⓐ owner localhost self-reference → ⓑ tunnel + token team sharing.** Read-only, single authorization point, transmission-boundary safe.

## Files created (Rust, in `knows_me_core` crate)

| File | Contents |
|---|---|
| `src-tauri/src/sharing/mod.rs` | Contract surface (Rust encoding of `mcp-contract.md`): `Token`, `SharingApi`, `MockSharing`, `AccessError`. Invariant §6: identity + grants come from `Token` only; consumer sees `granted ∩ {visibility == Shared}`; tool args only narrow. Re-exports `KnowledgeSharing`, `IssuedToken`/`TokenInfo`/`TokenStore`, `QuickTunnel`/`TunnelHandle`. |
| `src-tauri/src/sharing/knowledge.rs` | `KnowledgeSharing` — the real-store `SharingApi` over `Arc<KnowledgeService>` (owner mode ⓐ end-to-end). Rich methods reusing the single `accessible()` predicate: `category_counts` (§3.1 page_count, visible-only), `guide_pages` (§3.4), `search_visible` (returns full `Fact`, no double read). Frozen `SharingApi` signatures unchanged. |
| `src-tauri/src/sharing/mcp.rs` | Directly-implemented axum Streamable HTTP MCP server (chose direct impl over `rmcp` for a read-only 4-tool surface — consistent with the existing `LocalApiServer`, avoids a heavy dep tree). JSON-RPC 2.0 (`initialize`/`tools/list`/`tools/call`/`ping`), `/mcp` POST, `DEFAULT_PORT = 8766` (separate from persona 8765, §7.1). Identity seam `resolve_identity(tokens, headers, allow_owner)`; owner/shared listeners `start_owner` / `start_shared`. |
| `src-tauri/src/sharing/envelope.rs` | Serving-safety kernel (§4). `envelope()` — wraps human text in `<knows-me:content>…</knows-me:content>` and HTML-escapes any inner delimiter (invariant: exactly one real terminator pair in the output, proven by proptest). `sanitize_field()` — strips newlines/control chars + escapes delimiters for short fields. `NOT_INSTRUCTIONS` (§4.2 canonical Korean phrase). |
| `src-tauri/src/sharing/tokens.rs` | `TokenStore` over `EncryptedStore` (ns `"sharing-tokens"`). `issue(id, granted)` = 256-bit `OsRng` secret → base64url, **secret returned once**, record keyed by `base64url(SHA-256(secret))` (plaintext never stored, not even as the key). `resolve(secret)` = O(1) lookup → `Token::consumer` or `None`. `revoke(id)` (idempotent, fault-tolerant scan). `list()` for the issuance UI. |
| `src-tauri/src/sharing/tunnel.rs` | `QuickTunnel` / `TunnelHandle` — cloudflared quick-tunnel subprocess manager, parses the `https://<random>.trycloudflare.com` URL (`extract_tunnel_url`), distinguishes missing-binary / early-exit / timeout. Bridges the **shared listener only**. `CLOUDFLARED_BINARY` env override for a fake binary in unit tests. |

## Files created / modified (app shell + frontend)

| File | Change |
|---|---|
| `src-tauri/src/core/types.rs` | `AppConfig.sharing_enabled: bool` (opt-in, default `false`). |
| `src-tauri/src/core/commands.rs` | `set_sharing_enabled(state, on)` — flips config. |
| `desktop/src/services.rs` | `ServiceSet` spins the owner + shared MCP listeners on unlock, gated on `sharing_enabled`; `Services::set_sharing_enabled` for idempotent per-listener start/stop; shared listener binds owner-port + 1 to avoid collision; `stop_servers` on lock. |
| `desktop/src/main.rs` | Tauri commands registered: `set_sharing_enabled`, `share_status`, `issue_token`, `revoke_token`, `list_tokens`, `start_tunnel`, `stop_tunnel`. |
| `src/features/sharing/SharingSettings.tsx` (+ `.test.tsx`, `sharing.css`) | Settings "공유" tab — MCP on/off, tunnel start/stop + address copy, token issue (secret shown once) / revoke / list; granted categories from the owner's `list_categories`. |

## Contract coverage (`docs/mcp-contract.md`)

- **§1 두 모드** — owner (loopback, no token) vs consumer (tunnel + Bearer). `allow_owner` flag decides per-listener; **no Host-string identity判別** (the owner gate is a listener attribute, not a `Host` check).
- **§2 공유 타입** — consumes `core::types::{Visibility, Category}` and `FactMetadata.visibility/category` (frozen in PR #6).
- **§3 툴 4종** — `list_categories` / `search_knowledge` / `get_page` / `get_guide`, all read-only, all scoped by the single predicate.
- **§4 서빙 안전** — envelope + delimiter escaping + `NOT_INSTRUCTIONS` on every tool description. Redaction is not re-run (§4.3 — done at ingestion).
- **§5 에러** — `AccessError` → tool-result `isError` with the fixed Korean messages; protocol errors → JSON-RPC error; auth/store → HTTP 401/503.
- **§6 단일 인가 지점** — tool handlers do **zero** visibility/category re-checks; scope is fixed by `Token::can_access` before the tool sees the data. Owner mode uses the same path (grant = all, no visibility filter).
- **§7.1 분리** — MCP server is a separate port + code path from persona `LocalApiServer` (8765); only the MCP shared listener is bridged to the tunnel.

## Resolved open items (`mcp-contract.md` §10)

- **§10.3 터널 방식** — cloudflared quick tunnel (`brew install cloudflared`); token in the `Authorization: Bearer` header only, never in the URL. Store key = `base64url(SHA-256(secret))`.
- **§10.4 page_count 성능** — counted per request (personal scale = acceptable). A `MetaLite` RAM cache of `visibility`+`category` is the noted future optimization (needs `category` on `FactSummary`).
- **§10.1 범주 summary** — still open; served as `null` (field present).
- **§10.2 updated_at 정의** — still open (U3-owned); temporarily consumes `confirmed_at`, value swapped when U3 defines it.

## Security decisions (finalized)

- Owner = loopback + no Bearer on the **owner listener only**; consumer = Bearer (Host-agnostic, so the tunnel passes config-free); no-Bearer + non-loopback / shared listener → `401` with `WWW-Authenticate: Bearer`.
- Vault `Locked` / store error → HTTP `503` (distinct from `401`, so a consumer doesn't misread it as an invalid token and can relay it to a human).
- Listener split (review #1 BLOCKING) closes the DNS-rebinding / owner-via-tunnel latent exposure: the tunnel is never bridged to an owner-granting listener.

## Tests

- **57 test functions in `src/sharing/`** (verified present): envelope 16, knowledge 15, mcp 9, mod 6, tokens 9, tunnel 2 — plus proptest invariants (envelope terminator uniqueness, JSON round-trips) and HTTP-level smoke (consumer Bearer → granted `Shared` only; `Private`/non-granted → `not_found`; revoked/unknown → 401 with no leak).
- The vertical landed green: PR #13 (merge `83110bb`) recorded `cargo test --lib` **289 passed**, changed files clippy `-D warnings` / fmt clean, front-end `tsc` clean + vitest 83 passed, and a **live cloudflared tunnel smoke** (real `*.trycloudflare.com` URL: Bearer → granted `Shared`, `Private` → `not_found`, tokenless/revoked → 401).

## How to run

```
cd src-tauri
cargo test --lib                              # sharing tests included
cargo clippy --all-targets -- -D warnings     # clean (pre-existing ingestion/ warning is unrelated)
cargo fmt --check
```

Front-end: `npm test` (vitest). App: `npx tauri dev`, then Settings → 공유 tab.

## Operating it (owner → team)

Settings "공유" 탭 → MCP 켜기 → 터널 시작 → 토큰 발급(시크릿 1회 표시) → 팀원에게 URL + 토큰 전달.
Teammate: `claude mcp add --transport http <name> https://<tunnel>/mcp --header "Authorization: Bearer <token>"`.
Requires `cloudflared` installed.

## PR trail

Contract frozen: **PR #6**. Vertical: **PR #7** (real `SharingApi`, owner mode ⓐ) → **PR #9** (MCP transport + envelope kernel) → **PR #11** (consumer token auth ⓑ) → **PR #13** (listener split + app wiring + tunnel + issuance/status UI). Each merged after `/code-review` xhigh.

## Known / deferred

- **MCP/persona loopback lifecycle helper** — extracting a shared `bind`/`handle`/`stop` helper touches U4's `local_api.rs`; deferred, needs coordination (review finding #6).
- **§10.1 category summary / §10.2 `updated_at`** — U3-owned定義 pending; MCP consumes values only.
- **Read hot-path decryption cost at scale** — `list_categories`/`get_guide` decrypt all facts, `search` decrypts per candidate; acceptable at personal scale, resolved later by a `MetaLite` `visibility`+`category` RAM cache in U3's `SearchIndex`.
