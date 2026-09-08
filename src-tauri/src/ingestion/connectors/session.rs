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

        let mut items = Vec::new();
        let mut newest = since;

        for path in files {
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => return Err(AppError::Io(format!("stat {}: {e}", path.display()))),
            };
            let modified: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());

            // Incremental: skip files not newer than the cursor.
            if let Some(s) = since {
                if modified <= s {
                    continue;
                }
            }

            let text = fs::read_to_string(&path)
                .map_err(|e| AppError::Io(format!("read {}: {e}", path.display())))?;

            items.push(RawItem {
                source: SourceKind::Session,
                external_id: Self::external_id(&path),
                collected_at: Utc::now(),
                text: Some(text),
                image_png: None,
            });

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
