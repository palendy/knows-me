//! Embedded local REST server — US-6.2 ("나 대신 네트워킹").
//!
//! MVP policy (D7/D8): loopback only, no authentication, alive only while the
//! app runs. Exposure is prevented two ways, not one:
//!
//! 1. the listener binds `127.0.0.1` — there is no socket on any external
//!    interface to reach in the first place
//! 2. requests whose `Host` header is not loopback are rejected with 403,
//!    which is what stops DNS-rebinding from a browser page
//!
//! The handlers hold no business logic; they delegate to
//! [`PersonaApi`](crate::core::traits::PersonaApi) so chat and drafting have a
//! single implementation (BR-A3).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::core::error::{AppError, Result};
use crate::core::traits::PersonaApi;
use crate::core::types::{ChatRole, ChatTurn, DraftKind, DraftRequest};

/// Default port for the local persona API.
pub const DEFAULT_PORT: u16 = 8765;
/// How many ports above the default are tried when one is taken (BR-A7).
const PORT_SCAN_RANGE: u16 = 20;

// ---------------------------------------------------------------------------
// Wire DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRequestBody {
    pub prompt: String,
    /// Prior turns, oldest first. Optional so a one-shot client can omit it.
    #[serde(default)]
    pub history: Vec<ChatTurnBody>,
}

/// Wire form of [`ChatTurn`]. Declared here so U4 owns its HTTP contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurnBody {
    pub role: ChatRoleWire,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRoleWire {
    Owner,
    Persona,
}

impl From<ChatTurnBody> for ChatTurn {
    fn from(t: ChatTurnBody) -> Self {
        ChatTurn {
            role: match t.role {
                ChatRoleWire::Owner => ChatRole::Owner,
                ChatRoleWire::Persona => ChatRole::Persona,
            },
            text: t.text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRequestBody {
    pub kind: DraftKindWire,
    pub prompt: String,
}

/// Wire form of [`DraftKind`]. Declared here rather than deriving on the shared
/// type so U4 owns its own HTTP contract and U1's enum stays untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftKindWire {
    Email,
    Message,
    Post,
}

impl From<DraftKindWire> for DraftKind {
    fn from(w: DraftKindWire) -> Self {
        match w {
            DraftKindWire::Email => DraftKind::Email,
            DraftKindWire::Message => DraftKind::Message,
            DraftKindWire::Post => DraftKind::Post,
        }
    }
}

impl From<DraftKind> for DraftKindWire {
    fn from(k: DraftKind) -> Self {
        match k {
            DraftKind::Email => DraftKindWire::Email,
            DraftKind::Message => DraftKindWire::Message,
            DraftKind::Post => DraftKindWire::Post,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextResponseBody {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthBody {
    pub status: String,
    pub persona: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Error mapping (BR-A6) — the single source of truth for HTTP status codes
// ---------------------------------------------------------------------------

/// Map a domain error onto its HTTP status.
///
/// The message is the error's own `Display` text, which by construction carries
/// no file paths or stack traces (U4-NFR-SEC6).
pub fn status_for(err: &AppError) -> StatusCode {
    match err {
        AppError::Locked => StatusCode::LOCKED,
        AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        AppError::NotFound(_) => StatusCode::NOT_FOUND,
        AppError::External(_) => StatusCode::BAD_GATEWAY,
        AppError::Crypto(_) | AppError::Io(_) | AppError::Serde(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Every way a request can fail. Kept small on purpose so handlers can return
/// it by value without dragging a full `Response` through every `Result`.
enum ApiError {
    /// `Host` header was not loopback (BR-A2).
    NonLoopbackHost,
    /// Body was not the JSON shape the endpoint expects.
    MalformedBody,
    /// The persona service itself failed.
    Domain(AppError),
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError::Domain(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            // Says nothing about what the server holds (U4-NFR-SEC6).
            ApiError::NonLoopbackHost => (StatusCode::FORBIDDEN, "로컬 전용 API입니다".to_string()),
            ApiError::MalformedBody => (
                StatusCode::BAD_REQUEST,
                "요청 본문을 해석할 수 없습니다".to_string(),
            ),
            ApiError::Domain(e) => (status_for(&e), e.to_string()),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

// ---------------------------------------------------------------------------
// Host validation (BR-A2)
// ---------------------------------------------------------------------------

/// Whether a `Host` header value names the loopback interface.
///
/// The port suffix is ignored; anything else (a hostname, a LAN IP, a rebound
/// domain) is rejected.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    let name = if let Some(rest) = host.strip_prefix('[') {
        // IPv6 literal: [::1]:8765
        match rest.split_once(']') {
            Some((inner, _)) => inner,
            None => return false,
        }
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

type SharedPersona = Arc<dyn PersonaApi>;

/// Gate every handler on the `Host` header (BR-A2).
fn require_loopback(headers: &HeaderMap) -> std::result::Result<(), ApiError> {
    let ok = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_loopback_host);

    if ok {
        Ok(())
    } else {
        Err(ApiError::NonLoopbackHost)
    }
}

type JsonBody<T> = std::result::Result<Json<T>, axum::extract::rejection::JsonRejection>;

async fn health(headers: HeaderMap) -> std::result::Result<Json<HealthBody>, ApiError> {
    require_loopback(&headers)?;
    Ok(Json(HealthBody {
        status: "ok".into(),
        persona: true,
    }))
}

async fn chat(
    State(persona): State<SharedPersona>,
    headers: HeaderMap,
    body: JsonBody<ChatRequestBody>,
) -> std::result::Result<Json<TextResponseBody>, ApiError> {
    require_loopback(&headers)?;
    let Json(body) = body.map_err(|_| ApiError::MalformedBody)?;

    let history = body.history.into_iter().map(ChatTurn::from).collect();
    let reply = persona.chat(body.prompt, history).await?;
    Ok(Json(TextResponseBody { text: reply.text }))
}

async fn draft(
    State(persona): State<SharedPersona>,
    headers: HeaderMap,
    body: JsonBody<DraftRequestBody>,
) -> std::result::Result<Json<TextResponseBody>, ApiError> {
    require_loopback(&headers)?;
    let Json(body) = body.map_err(|_| ApiError::MalformedBody)?;

    let draft = persona
        .draft(DraftRequest {
            kind: body.kind.into(),
            prompt: body.prompt,
        })
        .await?;
    Ok(Json(TextResponseBody { text: draft.text }))
}

fn router(persona: SharedPersona) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/chat", post(chat))
        .route("/draft", post(draft))
        .with_state(persona)
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Starts the loopback-only persona API.
pub struct LocalApiServer;

/// A running server, owning its own shutdown.
///
/// Dropping the handle **stops** the server: the shutdown `oneshot::Sender`
/// drops with it, the receiver resolves, and graceful shutdown runs. That is
/// deliberate — a handle that outlives its owner would leave a listening socket
/// behind with nothing able to close it. For an orderly stop that waits for
/// in-flight requests to finish, call [`LocalApiHandle::stop`], which is safe
/// to call more than once.
pub struct LocalApiHandle {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl LocalApiHandle {
    /// The port actually bound — may differ from the requested one (BR-A7).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Signal graceful shutdown and wait for the listener to close.
    ///
    /// Idempotent: both the sender and the join handle are taken, so a second
    /// call is a no-op rather than a panic on an already-completed task.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            task.await.ok();
        }
    }
}

impl LocalApiServer {
    /// Bind and serve on `127.0.0.1`.
    ///
    /// If `port` is taken, ports up to `port + 20` are tried before giving up
    /// (BR-A7). The bound port is reported back through the handle.
    pub async fn start(persona: SharedPersona, port: u16) -> Result<LocalApiHandle> {
        let listener = bind_loopback(port).await?;
        let bound = listener
            .local_addr()
            .map_err(|e| AppError::Io(e.to_string()))?
            .port();

        let (tx, rx) = oneshot::channel::<()>();
        let app = router(persona);

        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });

        Ok(LocalApiHandle {
            port: bound,
            shutdown: Some(tx),
            task: Some(task),
        })
    }
}

/// Bind the first free loopback port at or above `start_port`.
///
/// `Ipv4Addr::LOCALHOST` is not configurable on purpose — U4-NFR-SEC1 forbids
/// binding any externally reachable interface.
async fn bind_loopback(start_port: u16) -> Result<TcpListener> {
    let mut last_err = None;
    for offset in 0..=PORT_SCAN_RANGE {
        let port = start_port.saturating_add(offset);
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        match TcpListener::bind(addr).await {
            Ok(l) => return Ok(l),
            Err(e) => last_err = Some(e),
        }
    }
    Err(AppError::Io(format!(
        "로컬 API 포트를 열 수 없습니다 ({start_port}..{}): {}",
        start_port.saturating_add(PORT_SCAN_RANGE),
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown".into())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{Draft, PersonaReply};
    use async_trait::async_trait;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    struct EchoPersona;

    #[async_trait]
    impl PersonaApi for EchoPersona {
        async fn chat(&self, prompt: String, history: Vec<ChatTurn>) -> Result<PersonaReply> {
            if prompt.trim().is_empty() {
                return Err(AppError::InvalidInput("빈 요청입니다".into()));
            }
            // Echo the history length so a test can prove it crossed the wire.
            Ok(PersonaReply {
                text: format!("chat:{prompt}:history={}", history.len()),
                sources: vec![],
            })
        }
        async fn draft(&self, req: DraftRequest) -> Result<Draft> {
            Ok(Draft {
                text: format!("draft:{:?}:{}", req.kind, req.prompt),
            })
        }
    }

    struct OfflinePersona;

    #[async_trait]
    impl PersonaApi for OfflinePersona {
        async fn chat(&self, _prompt: String, _history: Vec<ChatTurn>) -> Result<PersonaReply> {
            Err(AppError::External("offline".into()))
        }
        async fn draft(&self, _req: DraftRequest) -> Result<Draft> {
            Err(AppError::Locked)
        }
    }

    /// Minimal HTTP/1.1 client. Using raw TCP keeps the test dependency-free
    /// and lets us send an arbitrary `Host` header, which is the point of
    /// several of these tests.
    fn request(
        port: u16,
        method: &str,
        path: &str,
        host: &str,
        body: Option<&str>,
    ) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let body = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).expect("write");
        let mut raw = String::new();
        stream.read_to_string(&mut raw).expect("read");

        let status = raw
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    async fn serve(persona: SharedPersona) -> LocalApiHandle {
        // Port 0 lets the OS pick a free port, so tests never collide.
        LocalApiServer::start(persona, 0).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn chat_endpoint_returns_the_persona_reply() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, body) = tokio::task::spawn_blocking(move || {
            request(
                port,
                "POST",
                "/chat",
                "127.0.0.1",
                Some(r#"{"prompt":"안녕"}"#),
            )
        })
        .await
        .unwrap();

        assert_eq!(status, 200);
        let parsed: TextResponseBody = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.text, "chat:안녕:history=0");
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn chat_endpoint_forwards_prior_turns() {
        // A follow-up question is meaningless without them, so the wire format
        // has to carry them.
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, body) = tokio::task::spawn_blocking(move || {
            request(
                port,
                "POST",
                "/chat",
                "127.0.0.1",
                Some(
                    r#"{"prompt":"그거 더 자세히","history":[
                        {"role":"Owner","text":"배포 절차 알려줘"},
                        {"role":"Persona","text":"make deploy 입니다"}
                    ]}"#,
                ),
            )
        })
        .await
        .unwrap();

        assert_eq!(status, 200);
        let parsed: TextResponseBody = serde_json::from_str(&body).unwrap();
        assert!(parsed.text.ends_with("history=2"), "got: {}", parsed.text);
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn draft_endpoint_passes_the_kind_through() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, body) = tokio::task::spawn_blocking(move || {
            request(
                port,
                "POST",
                "/draft",
                "localhost",
                Some(r#"{"kind":"Email","prompt":"일정 공유"}"#),
            )
        })
        .await
        .unwrap();

        assert_eq!(status, 200);
        let parsed: TextResponseBody = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.text, "draft:Email:일정 공유");
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_loopback_host_is_rejected() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, body) = tokio::task::spawn_blocking(move || {
            request(
                port,
                "POST",
                "/chat",
                "evil.example.com",
                Some(r#"{"prompt":"안녕"}"#),
            )
        })
        .await
        .unwrap();

        assert_eq!(status, 403, "DNS-rebinding attempts must be refused");
        assert!(!body.contains("chat:"));
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn malformed_json_is_a_400() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, _) = tokio::task::spawn_blocking(move || {
            request(port, "POST", "/chat", "127.0.0.1", Some("{not json"))
        })
        .await
        .unwrap();

        assert_eq!(status, 400);
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn llm_outage_maps_to_502_and_lock_maps_to_423() {
        let mut h = serve(Arc::new(OfflinePersona)).await;
        let port = h.port();

        let (chat_status, draft_status) = tokio::task::spawn_blocking(move || {
            let c = request(
                port,
                "POST",
                "/chat",
                "127.0.0.1",
                Some(r#"{"prompt":"x"}"#),
            )
            .0;
            let d = request(
                port,
                "POST",
                "/draft",
                "127.0.0.1",
                Some(r#"{"kind":"Post","prompt":"x"}"#),
            )
            .0;
            (c, d)
        })
        .await
        .unwrap();

        assert_eq!(chat_status, 502);
        assert_eq!(draft_status, 423);
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_input_from_the_service_maps_to_400() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, _) = tokio::task::spawn_blocking(move || {
            request(
                port,
                "POST",
                "/chat",
                "127.0.0.1",
                Some(r#"{"prompt":"  "}"#),
            )
        })
        .await
        .unwrap();

        assert_eq!(status, 400);
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn health_reports_ok() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        let (status, body) =
            tokio::task::spawn_blocking(move || request(port, "GET", "/health", "127.0.0.1", None))
                .await
                .unwrap();

        assert_eq!(status, 200);
        let parsed: HealthBody = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.status, "ok");
        h.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stop_closes_the_listener_and_is_idempotent() {
        let mut h = serve(Arc::new(EchoPersona)).await;
        let port = h.port();

        h.stop().await;
        h.stop().await; // second call must not panic

        let refused = tokio::task::spawn_blocking(move || {
            TcpStream::connect_timeout(
                &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                std::time::Duration::from_millis(500),
            )
            .is_err()
        })
        .await
        .unwrap();

        assert!(refused, "port must be closed after stop()");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_the_handle_also_shuts_the_server_down() {
        let port = {
            let h = serve(Arc::new(EchoPersona)).await;
            h.port()
        }; // handle dropped here

        // Give the runtime a moment to run the shutdown future.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let refused = tokio::task::spawn_blocking(move || {
            TcpStream::connect_timeout(
                &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                std::time::Duration::from_millis(500),
            )
            .is_err()
        })
        .await
        .unwrap();

        assert!(
            refused,
            "dropping the handle must not leave a listening socket behind"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_busy_port_falls_through_to_the_next_one() {
        let first = LocalApiServer::start(Arc::new(EchoPersona), 0)
            .await
            .unwrap();
        let busy = first.port();

        let mut second = LocalApiServer::start(Arc::new(EchoPersona), busy)
            .await
            .unwrap();

        assert_ne!(second.port(), busy);
        assert!(second.port() > busy);

        second.stop().await;
        let mut first = first;
        first.stop().await;
    }

    #[test]
    fn loopback_host_detection() {
        for good in [
            "127.0.0.1",
            "127.0.0.1:8765",
            "localhost",
            "localhost:8765",
            "[::1]:8765",
        ] {
            assert!(is_loopback_host(good), "{good} should be accepted");
        }
        for bad in [
            "evil.example.com",
            "192.168.0.5:8765",
            "127.0.0.1.evil.com",
            "",
        ] {
            assert!(!is_loopback_host(bad), "{bad} should be rejected");
        }
    }

    #[test]
    fn error_status_mapping_is_exhaustive_and_stable() {
        assert_eq!(status_for(&AppError::Locked), StatusCode::LOCKED);
        assert_eq!(
            status_for(&AppError::InvalidInput("x".into())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_for(&AppError::NotFound("x".into())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for(&AppError::External("x".into())),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            status_for(&AppError::Io("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
