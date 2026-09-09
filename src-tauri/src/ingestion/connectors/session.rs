//! Session connector — collects agent session transcripts (Claude Code / Codex)
//! from local files. **Full implementation** (US-1.1).
//!
//! - Path: auto-detected known locations, overridable via `SourceConfig` (Q2=A).
//! - `external_id` = a stable id derived from the file path (dedup key with
//!   `SourceKind::Session`).
//! - Incremental: the cursor stores the RFC3339 timestamp of the newest file
//!   seen so far; only strictly-newer files are returned (BR-I, NFR-5).
//!
//! Local-only source, so there is no "mine" filter (BR-C1 N/A here).

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::core::error::{AppError, Result};
use crate::core::traits::Connector;
use crate::core::types::{Cursor, RawItem, SourceConfig, SourceKind};

/// How many transcripts one sync run collects.
///
/// Every collected item costs an LLM round trip in Processing, so an unbounded
/// run over a long-lived transcript directory (hundreds of files) would be slow
/// and expensive. Files are taken oldest-first and the cursor advances only to
/// the newest file actually included, so repeated runs march forward instead of
/// re-reading or skipping.
const MAX_FILES_PER_SYNC: usize = 30;

/// Character cap per transcript, keeping the tail.
///
/// A long session extracts to hundreds of KB even after the noise is stripped —
/// far past what one model call can take. The tail is kept because the end of a
/// session is where conclusions and decisions live.
const MAX_CHARS_PER_ITEM: usize = 24_000;

/// Reads session transcripts from one or more root directories.
pub struct SessionConnector {
    roots: Vec<PathBuf>,
}

impl SessionConnector {
    /// Construct from explicit roots (e.g. resolved from `SourceConfig`).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// Auto-detect known session locations, then apply any `SourceConfig`
    /// override (Q2=A). `config.0` may be `{"roots": ["/abs/path", ...]}`.
    pub fn from_config(config: Option<&SourceConfig>) -> Self {
        if let Some(cfg) = config {
            if let Some(roots) = cfg.0.get("roots").and_then(|v| v.as_array()) {
                let parsed: Vec<PathBuf> = roots
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(PathBuf::from)
                    .collect();
                if !parsed.is_empty() {
                    return Self::new(parsed);
                }
            }
        }
        Self::new(Self::default_roots())
    }

    /// Known default locations. Missing dirs are simply skipped at scan time.
    fn default_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            roots.push(home.join(".claude/projects")); // Claude Code transcripts
            roots.push(home.join(".codex/sessions")); // Codex sessions
        }
        roots
    }

    /// Stable id for a transcript file (dedup key). Path is stable per session.
    fn external_id(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn parse_cursor(cursor: Option<Cursor>) -> Option<DateTime<Utc>> {
        cursor
            .and_then(|c| DateTime::parse_from_rfc3339(&c.0).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Pull the conversation out of a Claude Code / Codex `.jsonl` transcript.
    ///
    /// A raw transcript is mostly machinery: `thinking` blocks carry multi-KB
    /// base64 signatures, and tool calls/results repeat file contents. On real
    /// sessions that is 93-98% of the bytes. Only `user` / `assistant` text
    /// survives here.
    ///
    /// Anything that does not parse as this shape is returned unchanged, so
    /// plain-text logs still work.
    fn extract_transcript(raw: &str) -> String {
        let mut turns = Vec::new();
        let mut parsed_any = false;

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            parsed_any = true;

            let Some(message) = value.get("message") else {
                continue;
            };
            let role = match message.get("role").and_then(|r| r.as_str()) {
                Some(r @ ("user" | "assistant")) => r,
                _ => continue,
            };

            match message.get("content") {
                Some(serde_json::Value::String(text)) if !text.trim().is_empty() => {
                    turns.push(format!("{role}: {text}"));
                }
                Some(serde_json::Value::Array(blocks)) => {
                    for block in blocks {
                        // `thinking`, `tool_use` and `tool_result` blocks are
                        // the bulk of the bytes and none of the meaning.
                        if block.get("type").and_then(|t| t.as_str()) != Some("text") {
                            continue;
                        }
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !text.trim().is_empty() {
                                turns.push(format!("{role}: {text}"));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if !parsed_any {
            return raw.to_string();
        }
        turns.join("\n")
    }

    /// Last `max` characters, on a character boundary (transcripts are UTF-8
    /// and frequently Korean, so byte slicing would panic).
    fn tail_chars(text: &str, max: usize) -> String {
        let count = text.chars().count();
        if count <= max {
            return text.to_string();
        }
        text.chars().skip(count - max).collect()
    }

    /// Recursively collect regular files under a root.
    fn scan_dir(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return; // unreadable dir → skip (caller records the source error)
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::scan_dir(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
}

#[async_trait]
impl Connector for SessionConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Session
    }

    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        let since = Self::parse_cursor(cursor);

        let mut files = Vec::new();
        for root in &self.roots {
            if root.exists() {
                Self::scan_dir(root, &mut files);
            }
        }

        // Pair each file with its mtime, dropping anything the cursor has
        // already covered.
        let mut candidates: Vec<(PathBuf, DateTime<Utc>)> = Vec::new();
        for path in files {
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => return Err(AppError::Io(format!("stat {}: {e}", path.display()))),
            };
            let modified: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());

            if let Some(s) = since {
                if modified <= s {
                    continue;
                }
            }
            candidates.push((path, modified));
        }

        // Oldest first, then take a bounded slice. Advancing the cursor only to
        // the newest file we actually included is what lets the next run pick up
        // exactly where this one stopped.
        candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        candidates.truncate(MAX_FILES_PER_SYNC);

        let mut items = Vec::new();
        let mut newest = since;

        for (path, modified) in candidates {
            let raw = fs::read_to_string(&path)
                .map_err(|e| AppError::Io(format!("read {}: {e}", path.display())))?;
            let text = Self::tail_chars(&Self::extract_transcript(&raw), MAX_CHARS_PER_ITEM);

            // A transcript with no conversation in it (tooling-only session)
            // carries nothing to learn from, but its mtime still counts as
            // covered so the cursor moves past it.
            if !text.trim().is_empty() {
                items.push(RawItem {
                    source: SourceKind::Session,
                    external_id: Self::external_id(&path),
                    collected_at: Utc::now(),
                    text: Some(text),
                    image_png: None,
                });
            }

            newest = Some(match newest {
                Some(n) if n >= modified => n,
                _ => modified,
            });
        }

        // Advance cursor to the newest modification time observed (or keep prior).
        let next = newest
            .map(|dt| dt.to_rfc3339())
            .or_else(|| since.map(|dt| dt.to_rfc3339()))
            .unwrap_or_default();

        Ok((items, Cursor(next)))
    }

    fn supports_manual(&self) -> bool {
        true // "지금 수집" also runs the session sync (US-1.2).
    }
}

#[cfg(test)]
mod extraction_tests {
    use super::*;

    fn line(role: &str, content: &str) -> String {
        format!(r#"{{"type":"{role}","message":{{"role":"{role}","content":"{content}"}}}}"#)
    }

    #[test]
    fn keeps_conversation_and_drops_the_machinery() {
        let raw = [
            r#"{"type":"pr-link","prNumber":"2"}"#.to_string(),
            line("user", "배포 절차 알려줘"),
            // An assistant turn: one huge thinking block, one real answer.
            r#"{"type":"assistant","message":{"role":"assistant","content":[
                {"type":"thinking","thinking":"","signature":"AAAAAAAAAAAAAAAAAAAAAAAA"},
                {"type":"tool_use","name":"Bash","input":{"command":"ls"}},
                {"type":"text","text":"main 머지 후 make deploy 입니다"}
            ]}}"#
                .replace('\n', " "),
        ]
        .join("\n");

        let out = SessionConnector::extract_transcript(&raw);

        assert!(out.contains("user: 배포 절차 알려줘"));
        assert!(out.contains("assistant: main 머지 후 make deploy 입니다"));
        assert!(!out.contains("signature"), "thinking noise must be dropped");
        assert!(!out.contains("tool_use"), "tool calls must be dropped");
        assert!(
            !out.contains("pr-link"),
            "non-message lines must be dropped"
        );
    }

    #[test]
    fn plain_text_logs_pass_through_unchanged() {
        let raw = "이건 JSONL이 아니라 그냥 텍스트 로그입니다.\n두 번째 줄.";
        assert_eq!(SessionConnector::extract_transcript(raw), raw);
    }

    #[test]
    fn a_transcript_with_no_conversation_extracts_to_nothing() {
        let raw = r#"{"type":"queue-operation","op":"x"}"#;
        assert!(SessionConnector::extract_transcript(raw).trim().is_empty());
    }

    #[test]
    fn tail_is_kept_and_never_splits_a_character() {
        // Korean is multi-byte: a byte-wise tail would panic or produce mojibake.
        let text: String = "가나다라마바사아자차".chars().cycle().take(100).collect();
        let tail = SessionConnector::tail_chars(&text, 10);

        assert_eq!(tail.chars().count(), 10);
        assert!(
            text.ends_with(&tail),
            "the *end* of the session is what matters"
        );
        assert_eq!(SessionConnector::tail_chars("짧음", 10), "짧음");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_root() -> PathBuf {
        // Unique-per-test dir without Math.random: use process id + a counter file.
        let base = std::env::temp_dir().join(format!("km_session_{}", std::process::id()));
        let _ = fs::create_dir_all(&base);
        base
    }

    #[tokio::test]
    async fn scans_and_is_incremental() {
        let root = tmp_root().join("scan1");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let f = root.join("s1.jsonl");
        let mut fh = fs::File::create(&f).unwrap();
        writeln!(fh, "hello session").unwrap();
        drop(fh);

        let conn = SessionConnector::new(vec![root.clone()]);
        let (items, cursor) = conn.sync(None).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, SourceKind::Session);
        assert!(items[0].text.as_deref().unwrap().contains("hello session"));
        assert!(!cursor.0.is_empty());

        // Re-sync with the returned cursor → nothing new (incremental).
        let (again, _) = conn.sync(Some(cursor)).await.unwrap();
        assert_eq!(again.len(), 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn missing_root_is_empty_not_error() {
        let conn = SessionConnector::new(vec![PathBuf::from("/nonexistent/km/path")]);
        let (items, _) = conn.sync(None).await.unwrap();
        assert_eq!(items.len(), 0);
    }
}
