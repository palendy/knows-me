//! File connector — watch-folder + manual upload (US-1.5).
//!
//! **Full**: text/markdown body parsing; unsupported → skip + reason (BR-F2).
//! **INTEGRATION-TODO**: PDF/DOCX parsing and image (vision) handling. The
//! branches exist and route correctly, but the heavy parsers are wired up during
//! integration (see `Cargo.toml` INTEGRATION-TODO deps). Images are emitted as a
//! `RawItem` with `image_png` set so Processing's vision path can extract text.
//!
//! Filesystem *watching* (`notify` crate) is also INTEGRATION-TODO; today files
//! are ingested via [`FileConnector::ingest_paths`] (manual upload / batch scan),
//! which is the same pipeline the watcher will feed (BR-F3).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::Result;
use crate::core::traits::{Connector, ProgressReporter};
use crate::core::types::{Cursor, RawItem, SourceKind};

/// Recognized file categories (Q7=A / BR-F1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFormat {
    Text,
    Office, // PDF / DOCX — INTEGRATION-TODO parsing
    Image,  // PNG / JPG — INTEGRATION-TODO vision (handled in Processing)
    Unsupported,
}

/// A skipped file and the reason (US-1.5 AC3). Surfaced for transparency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSkipRecord {
    pub path: String,
    pub reason: String,
}

/// Classify by extension.
pub fn classify_format(path: &Path) -> FileFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("txt") | Some("md") | Some("markdown") | Some("text") => FileFormat::Text,
        Some("pdf") | Some("docx") => FileFormat::Office,
        Some("png") | Some("jpg") | Some("jpeg") => FileFormat::Image,
        _ => FileFormat::Unsupported,
    }
}

/// File connector. Holds the batch of paths to ingest on the next `sync`
/// (populated by manual upload or, later, the `notify` watcher).
#[derive(Default)]
pub struct FileConnector {
    pending_paths: Mutex<Vec<PathBuf>>,
    /// Skips from the most recent `sync`, for transparency/debugging.
    pub last_skips: Mutex<Vec<FileSkipRecord>>,
}

impl FileConnector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue paths for ingestion (manual upload or batch scan). Same pipeline as
    /// the future watcher (BR-F3).
    pub fn ingest_paths(&self, paths: impl IntoIterator<Item = PathBuf>) {
        self.pending_paths.lock().unwrap().extend(paths);
    }

    /// Stable dedup id for a file path.
    fn external_id(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// Turn one path into a `RawItem`, or `Err(reason)` to skip it.
    fn to_raw_item(path: &Path) -> std::result::Result<RawItem, String> {
        match classify_format(path) {
            FileFormat::Text => {
                let text = fs::read_to_string(path)
                    .map_err(|e| format!("read {}: {e}", path.display()))?;
                Ok(RawItem {
                    source: SourceKind::File,
                    external_id: Self::external_id(path),
                    collected_at: Utc::now(),
                    text: Some(text),
                    image_png: None,
                })
            }
            FileFormat::Image => {
                let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
                // Vision text extraction happens in Processing via LlmClient.
                Ok(RawItem {
                    source: SourceKind::File,
                    external_id: Self::external_id(path),
                    collected_at: Utc::now(),
                    text: None,
                    image_png: Some(bytes),
                })
            }
            FileFormat::Office => {
                // INTEGRATION-TODO(US-1.5): parse PDF via `pdf-extract`, DOCX via
                // `docx-rs` (see Cargo.toml). Until wired, skip with a clear reason
                // rather than emitting empty facts.
                Err(format!(
                    "office parsing not yet integrated (INTEGRATION-TODO): {}",
                    path.display()
                ))
            }
            FileFormat::Unsupported => Err(format!(
                "unsupported format: {}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )),
        }
    }
}

#[async_trait]
impl Connector for FileConnector {
    fn id(&self) -> SourceKind {
        SourceKind::File
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        _progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        let paths: Vec<PathBuf> = std::mem::take(&mut *self.pending_paths.lock().unwrap());
        let mut items = Vec::new();
        let mut skips = Vec::new();

        for path in paths {
            match Self::to_raw_item(&path) {
                Ok(item) => items.push(item),
                Err(reason) => skips.push(FileSkipRecord {
                    path: path.to_string_lossy().into_owned(),
                    reason,
                }),
            }
        }

        *self.last_skips.lock().unwrap() = skips;
        // File dedup is by (File, external_id) at the CursorStore layer; the
        // connector itself keeps no incremental cursor.
        Ok((items, Cursor::default()))
    }

    fn supports_manual(&self) -> bool {
        true
    }
}

// Helper for callers/tests that cannot easily create files but want to validate
// classification/skip behavior.
impl FileConnector {
    /// Classify raw (path, exists-as-text) without touching the filesystem.
    /// Returns the skip reason for unsupported/office formats.
    pub fn skip_reason_for(path: &Path) -> Option<String> {
        match classify_format(path) {
            FileFormat::Text | FileFormat::Image => None,
            FileFormat::Office => Some("office parsing not yet integrated".into()),
            FileFormat::Unsupported => Some("unsupported format".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::NoProgress;
    use std::io::Write;

    #[test]
    fn classify_by_extension() {
        assert_eq!(classify_format(Path::new("a.md")), FileFormat::Text);
        assert_eq!(classify_format(Path::new("a.txt")), FileFormat::Text);
        assert_eq!(classify_format(Path::new("a.png")), FileFormat::Image);
        assert_eq!(classify_format(Path::new("a.pdf")), FileFormat::Office);
        assert_eq!(classify_format(Path::new("a.xyz")), FileFormat::Unsupported);
    }

    #[tokio::test]
    async fn text_file_ingested_unsupported_skipped() {
        let dir = std::env::temp_dir().join(format!("km_file_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("note.md");
        let mut fh = fs::File::create(&txt).unwrap();
        writeln!(fh, "# heading\nbody").unwrap();
        drop(fh);
        let bad = dir.join("archive.zip");
        fs::File::create(&bad).unwrap();

        let conn = FileConnector::new();
        conn.ingest_paths([txt, bad]);
        let (items, _) = conn.sync(None, &NoProgress).await.unwrap();

        assert_eq!(items.len(), 1);
        assert!(items[0].text.as_deref().unwrap().contains("heading"));
        let skips = conn.last_skips.lock().unwrap();
        assert_eq!(skips.len(), 1);
        assert!(skips[0].reason.contains("unsupported"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn office_is_skipped_with_integration_reason() {
        let dir = std::env::temp_dir().join(format!("km_file_office_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("doc.pdf");
        fs::File::create(&pdf).unwrap();

        let conn = FileConnector::new();
        conn.ingest_paths([pdf]);
        let (items, _) = conn.sync(None, &NoProgress).await.unwrap();
        assert_eq!(items.len(), 0);
        assert!(conn.last_skips.lock().unwrap()[0]
            .reason
            .contains("INTEGRATION-TODO"));

        let _ = fs::remove_dir_all(&dir);
    }
}
