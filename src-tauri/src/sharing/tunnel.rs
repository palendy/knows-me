//! cloudflared quick-tunnel process manager (step ⓑ operations).
//!
//! Fronts the **shared** MCP listener ([`McpServer::start_shared`]) with a public
//! `https://<random>.trycloudflare.com` URL so a teammate's agent can reach it.
//! The tunnel is zero-config and account-less: it runs
//! `cloudflared tunnel --url http://127.0.0.1:<port>` and reads the assigned URL
//! back from the process output.
//!
//! Boundaries this holds to:
//! - **Bearer, never the URL (§10.3).** cloudflared only forwards bytes; the
//!   consumer token rides the `Authorization` header end-to-end. This module never
//!   puts a secret in a URL, and the tunnel URL itself is not a secret.
//! - **Shared listener only.** The caller bridges this onto the Bearer-only
//!   shared listener, never the owner listener — a tunnel may rewrite `Host` to
//!   loopback, which is exactly why the owner listener is never tunneled (see
//!   [`resolve_identity`](super::mcp)).
//!
//! [`McpServer::start_shared`]: super::mcp::McpServer::start_shared

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::error::{AppError, Result};

/// Environment override for the cloudflared binary (path or name). Lets an owner
/// point at a non-`PATH` install, and lets tests drive a stand-in binary.
const CLOUDFLARED_BINARY_ENV: &str = "CLOUDFLARED_BINARY";

/// How long to wait for cloudflared to announce its URL before giving up. A quick
/// tunnel normally reports within a few seconds; this is a generous ceiling.
const URL_TIMEOUT: Duration = Duration::from_secs(30);

/// The cloudflared binary to invoke: the env override, else `cloudflared` on the
/// `PATH`.
fn cloudflared_binary() -> String {
    std::env::var(CLOUDFLARED_BINARY_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "cloudflared".to_string())
}

/// Extract a quick-tunnel URL from one line of cloudflared output.
///
/// cloudflared prints the URL inside an ASCII box, e.g.
/// `INF |  https://calm-band-1234.trycloudflare.com  |`. We take the `https://`
/// token and accept it only when its host ends in `.trycloudflare.com`, so an
/// unrelated `https://` the tool may log (a docs link, an update notice) is not
/// mistaken for the tunnel address.
pub fn extract_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '|' | '"' | '\'' | '<' | '>'))
        .unwrap_or(rest.len());
    let url = &rest[..end];
    let host = url.strip_prefix("https://")?.split('/').next()?;
    let suffix = ".trycloudflare.com";
    (host.len() > suffix.len() && host.ends_with(suffix)).then(|| url.to_string())
}

/// Whether a cloudflared binary is runnable — a quick `--version` probe. Used by
/// the sharing status so the UI can tell "tunnel off" from "cloudflared not
/// installed" and point the owner at the fix.
pub async fn cloudflared_available() -> bool {
    Command::new(cloudflared_binary())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Starts a cloudflared quick tunnel.
pub struct QuickTunnel;

/// A running quick tunnel, owning the cloudflared child process. Dropping the
/// handle kills the child (`kill_on_drop`); [`TunnelHandle::stop`] does it
/// explicitly and is idempotent.
pub struct TunnelHandle {
    url: String,
    /// The cloudflared child. `Some` while running; taken by `stop`.
    child: Option<Child>,
    /// Line-draining tasks for the child's stdout/stderr. Aborted on stop; they
    /// also end on their own once the child dies and the pipes close.
    readers: Vec<JoinHandle<()>>,
}

impl TunnelHandle {
    /// The public `https://<random>.trycloudflare.com` URL fronting the shared
    /// listener. Not a secret — the token is what gates access.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Kill cloudflared and stop draining its output. Idempotent.
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        for r in self.readers.drain(..) {
            r.abort();
        }
    }
}

impl QuickTunnel {
    /// Spawn a quick tunnel fronting `127.0.0.1:port` and return once cloudflared
    /// has announced its public URL. Errors if the binary cannot be spawned
    /// (usually "not installed"), if cloudflared exits before announcing a URL, or
    /// if no URL arrives within [`URL_TIMEOUT`].
    pub async fn start(port: u16) -> Result<TunnelHandle> {
        Self::start_with(&cloudflared_binary(), port).await
    }

    /// [`start`](Self::start) with an explicit binary — the seam tests drive with
    /// a stand-in so spawn/parse/lifecycle are exercised without real cloudflared.
    async fn start_with(bin: &str, port: u16) -> Result<TunnelHandle> {
        let mut child = Command::new(bin)
            .arg("tunnel")
            .arg("--no-autoupdate")
            .arg("--url")
            .arg(format!("http://127.0.0.1:{port}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                AppError::Io(format!(
                    "cloudflared를 실행할 수 없습니다 ({bin}): {e}. 설치되어 있는지 확인하세요."
                ))
            })?;

        // cloudflared logs the URL to stderr in current builds; drain both streams
        // to be robust, and to keep the child from blocking on a full pipe once we
        // stop reading. Each reader forwards only a matched URL line, then keeps
        // draining silently.
        let (tx, mut rx) = mpsc::channel::<String>(8);
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(spawn_url_scanner(out, tx.clone()));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(spawn_url_scanner(err, tx.clone()));
        }
        drop(tx); // so `rx` closes when both readers end (i.e. the child exited)

        let deadline = tokio::time::sleep(URL_TIMEOUT);
        tokio::pin!(deadline);
        let url = loop {
            tokio::select! {
                line = rx.recv() => match line {
                    Some(l) => match extract_tunnel_url(&l) {
                        Some(u) => break u,
                        None => continue,
                    },
                    // Both pipes closed with no URL ⇒ cloudflared exited early.
                    None => {
                        return Err(AppError::External(
                            "cloudflared가 터널 URL을 내보내기 전에 종료됐습니다.".to_string(),
                        ));
                    }
                },
                _ = &mut deadline => {
                    let _ = child.start_kill();
                    return Err(AppError::External(
                        "cloudflared 터널 URL을 시간 내에 받지 못했습니다.".to_string(),
                    ));
                }
            }
        };

        Ok(TunnelHandle {
            url,
            child: Some(child),
            readers,
        })
    }
}

/// Read `reader` line by line to EOF, forwarding the first line that carries a
/// tunnel URL. Non-URL lines are discarded and, once the receiver is gone, so are
/// URL lines — the loop keeps reading either way so the child never stalls on a
/// full pipe.
fn spawn_url_scanner<R>(reader: R, tx: mpsc::Sender<String>) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        let mut tx = Some(tx);
        while let Ok(Some(line)) = lines.next_line().await {
            if extract_tunnel_url(&line).is_some() {
                if let Some(sender) = &tx {
                    if sender.send(line).await.is_err() {
                        // Receiver dropped (URL already found): stop sending but
                        // keep draining the pipe.
                        tx = None;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_url_from_cloudflared_box_output() {
        let line = "2026-09-09T10:00:00Z INF |  https://calm-band-1234.trycloudflare.com  |";
        assert_eq!(
            extract_tunnel_url(line).as_deref(),
            Some("https://calm-band-1234.trycloudflare.com")
        );
    }

    #[test]
    fn extract_url_ignores_unrelated_https_lines() {
        // Update notices / docs links must not be mistaken for the tunnel URL.
        assert_eq!(
            extract_tunnel_url("INF Visit https://developers.cloudflare.com for docs"),
            None
        );
        assert_eq!(extract_tunnel_url("no url here at all"), None);
        // A bare suffix with no subdomain is not a tunnel URL.
        assert_eq!(extract_tunnel_url("https://trycloudflare.com"), None);
    }

    #[cfg(unix)]
    fn fake_cloudflared(script: &str) -> (tempfile::TempDir, String) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloudflared");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let s = path.to_string_lossy().into_owned();
        (dir, s)
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn start_parses_url_and_stop_kills_the_child() {
        // A stand-in that prints a cloudflared-style URL line to stderr, then
        // stays alive so the handle has something to stop.
        let (_dir, bin) = fake_cloudflared(
            "#!/bin/sh\n\
             echo 'INF Thank you for trying Cloudflare Tunnel.' 1>&2\n\
             echo 'INF |  https://unit-test-abcd.trycloudflare.com  |' 1>&2\n\
             sleep 30\n",
        );

        let mut h = QuickTunnel::start_with(&bin, 8767).await.unwrap();
        assert_eq!(h.url(), "https://unit-test-abcd.trycloudflare.com");
        h.stop().await;
        h.stop().await; // idempotent
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn start_errors_when_cloudflared_exits_before_a_url() {
        let (_dir, bin) = fake_cloudflared("#!/bin/sh\necho 'boom' 1>&2\nexit 1\n");
        let err = QuickTunnel::start_with(&bin, 8767).await;
        assert!(err.is_err(), "an early exit with no URL must be an error");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn start_errors_when_the_binary_is_missing() {
        let err = QuickTunnel::start_with("/nonexistent/cloudflared-xyz", 8767).await;
        assert!(matches!(err, Err(AppError::Io(_))));
    }
}
