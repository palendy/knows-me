//! Session connector — collects agent session transcripts (Claude Code / Codex)
//! from local files. **Full implementation** (US-1.1).
//!
//! - Path: auto-detected known locations, overridable via `SourceConfig` (Q2=A).
//! - `external_id` = a stable id derived from the file path (dedup key with
//!   `SourceKind::Session`).
//! - Incremental: the cursor carries two watermarks (BR-I, NFR-5). New sessions
//!   are picked up first, newest-first; leftover budget backfills history from
//!   the frontier downward. A single "newest seen" watermark would have meant
//!   the first run processed the *oldest* files on disk — a directory that has
//!   accumulated for months starts with material the owner has long forgotten.
//!
//! Local-only source, so there is no "mine" filter (BR-C1 N/A here).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::core::error::{AppError, Result};
use crate::core::traits::{Connector, ProgressReporter};
use crate::core::types::{Cursor, RawItem, SourceConfig, SourceKind};

/// How many transcripts one sync run collects.
///
/// Every collected item costs an LLM round trip in Processing, so an unbounded
/// run over a long-lived transcript directory (hundreds of files) would be slow
/// and expensive. Files are taken oldest-first and the cursor advances only to
/// the newest file actually included, so repeated runs march forward instead of
/// re-reading or skipping.
const MAX_FILES_PER_SYNC: usize = 30;

/// Directories under the transcript roots that hold machine output rather than
/// the owner's own sessions.
///
/// `subagents/` alone is 573 of 715 files in a real installation: fan-out
/// workers whose transcripts are about the task they were handed, not about the
/// person. Ingesting them buries the owner's actual context.
const EXCLUDED_DIR_NAMES: &[&str] = &["subagents", "shell-snapshots", "todos", "statsig"];

/// Character cap per transcript, keeping the tail.
///
/// A long session extracts to hundreds of KB even after the noise is stripped —
/// far past what one model call can take. The tail is kept because the end of a
/// session is where conclusions and decisions live.
const MAX_CHARS_PER_ITEM: usize = 24_000;

/// Character budget for the owner's own words inside one digest.
///
/// This is the part of the digest that is actually *about the person* — why
/// they are doing something, what they are worried about, what they decided
/// and why. Everything else (tool profile, file profile, commands) describes
/// activity, and activity is the least useful thing extraction can produce.
///
/// It was previously a count — the last 12 turns, clipped to 400 characters
/// each. Measured against a real transcript directory, that kept a median of
/// **6.6% of what the owner had typed** while using **7.7% of the item budget
/// above**: 93% of the input was discarded to save room that was then left
/// empty. 73% of sessions ran past the 12-turn cap.
const PROMPT_BUDGET_CHARS: usize = 18_000;

/// Cap on a single turn, so one pasted document cannot eat the whole budget.
///
/// Generous on purpose: a turn explaining *why* — the reasoning this whole
/// pipeline exists to capture — routinely runs past a few hundred characters,
/// and the old 400-character clip cut those off mid-sentence.
const MAX_PROMPT_CHARS: usize = 3_000;

/// First line of each prompt this app itself sends to the Claude CLI backend.
///
/// The CLI backend drives Claude Code, which records the request as a user turn
/// in *its own* transcript — under the same directory this connector scans. So
/// every extraction knows-me runs becomes owner speech for the next collection
/// to extract from, and the knowledge base fills with facts about knows-me's
/// prompts. Observed in a real run: a stored Concept titled "사실 노트 분류
/// 스키마의 certainty 필드", and a queue item asking the owner to confirm a
/// complaint the model made about the prompt it had been handed.
///
/// Matching on the prompts' own first lines rather than a hand-copied string
/// keeps the filter correct when the prompts are reworded.
fn own_prompt_openers() -> [&'static str; 2] {
    [
        first_line(crate::llm::prompts::SUMMARIZE_SYSTEM),
        first_line(crate::llm::prompts::CLASSIFY_SYSTEM),
    ]
}

fn first_line(s: &str) -> &str {
    s.split('\n').next().unwrap_or(s).trim()
}

/// Record one of the owner's turns, trimmed and without the boilerplate that
/// the harness injects into user-role messages.
fn push_prompt(out: &mut Vec<String>, text: &str) {
    let text = text.trim();
    // Tool results and system reminders arrive under the user role but are not
    // the owner speaking.
    if text.is_empty()
        || text.starts_with("<system-reminder>")
        || text.starts_with("<command-")
        || text.starts_with("Caveat:")
    {
        return;
    }
    // Nor is this app talking to itself (see `own_prompt_openers`).
    if own_prompt_openers().iter().any(|o| text.starts_with(o)) {
        return;
    }
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = one_line.chars().take(MAX_PROMPT_CHARS).collect();
    if !clipped.is_empty() {
        out.push(clipped);
    }
}

/// Choose which of the owner's turns fit the budget, keeping both ends.
///
/// Taking only the tail was wrong for the thing this pipeline is for. A
/// session's *end* holds what was decided; its *beginning* holds why the owner
/// set out to do it and what was bothering them — and "why" is precisely what
/// a tail-only window drops. So turns are taken alternately from the front and
/// the back, front first, and the middle (iteration detail: retries, small
/// corrections) is what gives way when something has to.
///
/// Returns the kept turns in their original order, and how many were dropped
/// so the digest can say so rather than silently presenting a gap as continuous.
fn select_prompts(prompts: &[String], budget: usize) -> (Vec<&String>, usize) {
    let total: usize = prompts.iter().map(|p| p.chars().count() + 3).sum();
    if total <= budget {
        return (prompts.iter().collect(), 0);
    }

    let (mut head, mut tail) = (Vec::new(), Vec::new());
    let (mut lo, mut hi) = (0usize, prompts.len());
    let mut spent = 0usize;
    let mut take_front = true;
    while lo < hi {
        let idx = if take_front { lo } else { hi - 1 };
        let cost = prompts[idx].chars().count() + 3;
        if spent + cost > budget {
            break;
        }
        spent += cost;
        if take_front {
            head.push(&prompts[idx]);
            lo += 1;
        } else {
            tail.push(&prompts[idx]);
            hi -= 1;
        }
        take_front = !take_front;
    }

    let dropped = hi - lo;
    tail.reverse();
    head.extend(tail);
    (head, dropped)
}

/// Collapse an MCP tool id (`mcp__<server-uuid>__notion-update-page`) to
/// something readable; leave built-ins alone.
fn tool_label(name: &str) -> String {
    match name.strip_prefix("mcp__") {
        Some(rest) => match rest.split_once("__") {
            Some((_server, tool)) => format!("mcp:{tool}"),
            None => format!("mcp:{rest}"),
        },
        None => name.to_string(),
    }
}

/// Reduce a shell command to the program (plus subcommand for the multiplexers)
/// so counts show which tools someone actually lives in.
fn command_label(command: &str) -> String {
    let first = command
        .split(&['|', ';', '&'][..])
        .next()
        .unwrap_or(command)
        .trim();
    let mut parts = first.split_whitespace().filter(|t| !t.contains('='));
    let Some(mut program) = parts.next() else {
        return "?".into();
    };
    // `cd /somewhere && cargo test` — the interesting part is after the cd.
    if program == "cd" {
        return command
            .split("&&")
            .nth(1)
            .map(command_label)
            .unwrap_or_else(|| "cd".into());
    }
    if let Some(base) = program.rsplit('/').next() {
        program = base;
    }
    const MULTIPLEXERS: &[&str] = &[
        "git", "npm", "npx", "cargo", "docker", "pip", "python", "uv",
    ];
    if MULTIPLEXERS.contains(&program) {
        if let Some(sub) = parts.next() {
            if !sub.starts_with('-') {
                return format!("{program} {sub}");
            }
        }
    }
    program.to_string()
}

/// The directory the session's files sit under, if they share one.
///
/// A session's working project says more about the owner than any single file.
fn common_project(files: &BTreeMap<String, usize>) -> Option<String> {
    let mut iter = files.keys();
    let mut prefix: Vec<&str> = iter.next()?.split('/').collect();
    for path in iter {
        let parts: Vec<&str> = path.split('/').collect();
        let keep = prefix
            .iter()
            .zip(parts.iter())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(keep);
        if prefix.is_empty() {
            return None;
        }
    }
    // Drop the trailing filename component when every path was identical.
    if prefix.len() > 1 && prefix.last().is_some_and(|p| p.contains('.')) {
        prefix.pop();
    }
    let joined = prefix.join("/");
    (joined.len() > 1).then_some(joined)
}

/// `name (count) · name (count) · …`, most frequent first.
fn top_n(counts: &BTreeMap<String, usize>, n: usize) -> String {
    let mut pairs: Vec<(&String, &usize)> = counts.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    pairs
        .into_iter()
        .take(n)
        .map(|(k, v)| format!("{k} ({v})"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// One project directory the owner could choose to collect from.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionProject {
    /// Absolute path, and what `SourceConfig`'s `roots` expects back.
    pub path: String,
    /// The working directory the transcripts belong to, as best we can tell.
    ///
    /// Claude Code encodes it in the directory name by replacing separators
    /// with dashes, which is unreadable on screen; this restores something the
    /// owner recognises.
    pub label: String,
    pub sessions: usize,
}

/// A readable name for a project directory.
///
/// The directory name cannot be decoded: Claude Code builds it from the working
/// directory by replacing `/`, `_` and `.` all with `-`, so
/// `Work/18_avatar/knows-me` and `Work-18-avatar-knows-me` are the same string
/// and guessing produced nonsense like `Work/18/avatar/knows/me`.
///
/// The transcripts carry the real path in their `cwd` field, so we read it
/// instead of inferring it. One line of one file is enough; the dashed name is
/// the fallback for a directory whose transcripts predate that field.
fn project_label(dir: &Path, dir_name: &str) -> String {
    match read_cwd(dir) {
        Some(cwd) => shorten_path(&cwd),
        None => dir_name.trim_start_matches('-').replace('-', "/"),
    }
}

/// The working directory a project's transcripts belong to.
fn read_cwd(dir: &Path) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        // Read line by line, not the whole file: transcripts run to megabytes
        // and this only needs the first few records. The picker opens a dialog,
        // so slurping every project's largest transcript would stall it.
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        use std::io::BufRead;
        for line in std::io::BufReader::new(file)
            .lines()
            // `Result::ok` here would resolve to this crate's `Result` alias,
            // not `std`'s — spell the closure out.
            .map_while(|l| l.ok())
            .take(40)
        {
            if !line.contains("\"cwd\"") {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
                    if !cwd.is_empty() {
                        return Some(cwd.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Drop the home-directory prefix so entries do not all begin the same way.
///
/// Keeps the absolute path when there is nothing recognisable to trim — a
/// shortened label that hides which of two same-named folders this is would be
/// worse than a long one.
fn shorten_path(cwd: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().to_string();
        if let Some(rest) = cwd.strip_prefix(&home) {
            let rest = rest.trim_start_matches('/');
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    cwd.to_string()
}

/// Reads session transcripts from one or more root directories.
pub struct SessionConnector {
    /// Behind a lock because the owner re-scopes collection while the app runs
    /// and the registry hands out `Arc<dyn Connector>` — see
    /// [`Connector::configure`](crate::core::traits::Connector::configure).
    roots: std::sync::RwLock<Vec<PathBuf>>,
}

impl SessionConnector {
    /// Construct from explicit roots (e.g. resolved from `SourceConfig`).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: std::sync::RwLock::new(roots),
        }
    }

    fn roots(&self) -> Vec<PathBuf> {
        // A poisoned lock means a panic while re-scoping; the roots themselves
        // are still readable and collecting from stale roots beats refusing to
        // collect at all.
        self.roots.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Every project directory under the *default* locations, with counts, so
    /// the owner can pick a subset without typing paths.
    ///
    /// Deliberately scans the defaults rather than the configured roots: this
    /// answers "what could I collect?", and a narrowed scope must not hide the
    /// projects it left out.
    pub fn available_projects() -> Vec<SessionProject> {
        let mut out = Vec::new();
        for root in Self::default_roots() {
            let Ok(entries) = fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if EXCLUDED_DIR_NAMES.contains(&name.as_str()) {
                    continue;
                }
                let mut sessions = 0usize;
                if let Ok(files) = fs::read_dir(&path) {
                    for f in files.flatten() {
                        if f.path().extension().is_some_and(|e| e == "jsonl") {
                            sessions += 1;
                        }
                    }
                }
                if sessions == 0 {
                    continue;
                }
                out.push(SessionProject {
                    label: project_label(&path, &name),
                    path: path.to_string_lossy().into_owned(),
                    sessions,
                });
            }
        }
        out.sort_by(|a, b| {
            b.sessions
                .cmp(&a.sessions)
                .then_with(|| a.label.cmp(&b.label))
        });
        out
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
    ///
    /// On Windows the two agents usually live on different filesystems — Codex
    /// under the Windows profile, Claude Code under the WSL home — so we scan the
    /// Windows profile *and* every WSL distro's user homes (reached from Windows
    /// via the `\\wsl.localhost\<distro>\` share) in addition to `$HOME`.
    fn default_roots() -> Vec<PathBuf> {
        fn agent_dirs(base: &Path, out: &mut Vec<PathBuf>) {
            out.push(base.join(".claude/projects")); // Claude Code transcripts
            out.push(base.join(".codex/sessions")); // Codex sessions
        }

        let mut roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            agent_dirs(&PathBuf::from(home), &mut roots);
        }
        #[cfg(windows)]
        {
            if let Some(profile) = std::env::var_os("USERPROFILE") {
                agent_dirs(&PathBuf::from(profile), &mut roots);
            }
            for home in Self::wsl_home_dirs() {
                agent_dirs(&home, &mut roots);
            }
        }

        // HOME and USERPROFILE often coincide on Windows; drop duplicate roots so
        // a directory isn't scanned (and its items deduped) twice.
        let mut seen = std::collections::HashSet::new();
        roots.retain(|p| seen.insert(p.clone()));
        roots
    }

    /// Every `home/<user>` directory across installed WSL distros, reached from
    /// Windows via the `\\wsl.localhost\<distro>\` (or legacy `\\wsl$\`) share.
    ///
    /// The `\\wsl.localhost\` root itself can't be listed with `read_dir`, so we
    /// ask WSL for its distros (`wsl -l -q`, UTF-16LE output) and read each one's
    /// `home`. Best-effort: if WSL isn't present the command fails and we yield
    /// nothing.
    #[cfg(windows)]
    fn wsl_home_dirs() -> Vec<PathBuf> {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000; // don't flash a console

        let stdout = match Command::new("wsl.exe")
            .args(["--list", "--quiet"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(),
        };
        // Decode UTF-16LE (wsl.exe's output encoding).
        let listing: Vec<u16> = stdout
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let listing = String::from_utf16_lossy(&listing);

        let mut homes = Vec::new();
        for line in listing.lines() {
            let distro =
                line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0');
            if distro.is_empty() {
                continue;
            }
            for provider in [r"\\wsl.localhost", r"\\wsl$"] {
                let home = PathBuf::from(format!(r"{provider}\{distro}\home"));
                if let Ok(users) = fs::read_dir(&home) {
                    for user in users.flatten() {
                        homes.push(user.path());
                    }
                    break; // this provider served the distro
                }
            }
        }
        homes
    }

    /// Stable id for a transcript file (dedup key). Path is stable per session.
    fn external_id(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// Two watermarks: everything strictly newer than `newest` is unprocessed
    /// (forward sync), and everything strictly older than `oldest` is
    /// unprocessed (backfill). Between them is done.
    ///
    /// Reads the legacy single-timestamp form as `newest`, with no backfill
    /// frontier, so an existing vault keeps working.
    fn parse_cursor(cursor: Option<Cursor>) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let Some(Cursor(raw)) = cursor else {
            return (None, None);
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let at = |k: &str| {
                v.get(k)
                    .and_then(|x| x.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&Utc))
            };
            return (at("newest"), at("oldest"));
        }
        let legacy = DateTime::parse_from_rfc3339(&raw)
            .ok()
            .map(|d| d.with_timezone(&Utc));
        (legacy, None)
    }

    fn render_cursor(newest: Option<DateTime<Utc>>, oldest: Option<DateTime<Utc>>) -> Cursor {
        let mut obj = serde_json::Map::new();
        if let Some(n) = newest {
            obj.insert("newest".into(), n.to_rfc3339().into());
        }
        if let Some(o) = oldest {
            obj.insert("oldest".into(), o.to_rfc3339().into());
        }
        Cursor(serde_json::Value::Object(obj).to_string())
    }

    /// Whether a path sits under a directory that holds machine output.
    fn is_excluded(path: &Path) -> bool {
        path.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|name| EXCLUDED_DIR_NAMES.contains(&name))
        })
    }

    /// Condense a Claude Code / Codex `.jsonl` transcript into a signal digest.
    ///
    /// Feeding a transcript verbatim is the wrong shape twice over: the bytes
    /// are dominated by machinery (base64 `thinking` signatures, tool results
    /// echoing whole files — 93-98% of a real session), and what survives is
    /// still mostly the assistant talking, which says nothing about the owner.
    ///
    /// What actually characterises a person is kept instead: their own words,
    /// which projects they work in, which tools they reach for, which commands
    /// they run, and which files they keep returning to. That is a few KB per
    /// session rather than megabytes, and every line of it is about them.
    ///
    /// Input that is not JSONL is returned unchanged, so plain-text logs work.
    fn digest_transcript(raw: &str) -> String {
        let mut prompts: Vec<String> = Vec::new();
        let mut tool_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut command_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut file_counts: BTreeMap<String, usize> = BTreeMap::new();
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
            let role = message.get("role").and_then(|r| r.as_str());

            match message.get("content") {
                // A plain-string user turn is the owner speaking.
                Some(serde_json::Value::String(text)) if role == Some("user") => {
                    push_prompt(&mut prompts, text);
                }
                Some(serde_json::Value::Array(blocks)) => {
                    for block in blocks {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") if role == Some("user") => {
                                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                    push_prompt(&mut prompts, t);
                                }
                            }
                            Some("tool_use") => {
                                let name = block
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown");
                                *tool_counts.entry(tool_label(name)).or_insert(0) += 1;

                                let input = block.get("input");
                                if let Some(cmd) = input
                                    .and_then(|i| i.get("command"))
                                    .and_then(|c| c.as_str())
                                {
                                    *command_counts.entry(command_label(cmd)).or_insert(0) += 1;
                                }
                                for key in ["file_path", "path", "notebook_path"] {
                                    if let Some(path) =
                                        input.and_then(|i| i.get(key)).and_then(|p| p.as_str())
                                    {
                                        *file_counts.entry(path.to_string()).or_insert(0) += 1;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if !parsed_any {
            return raw.to_string();
        }

        let mut out = String::new();

        if let Some(project) = common_project(&file_counts) {
            out.push_str(&format!("[프로젝트]\n{project}\n\n"));
        }

        if !prompts.is_empty() {
            out.push_str("[사용자 발화]\n");
            let (kept, dropped) = select_prompts(&prompts, PROMPT_BUDGET_CHARS);
            for (i, p) in kept.iter().enumerate() {
                if dropped > 0 && i == kept.len() / 2 {
                    out.push_str(&format!("- (중략: 발화 {dropped}개)\n"));
                }
                out.push_str(&format!("- {p}\n"));
            }
            out.push('\n');
        }

        if !tool_counts.is_empty() {
            out.push_str("[도구 사용]\n");
            out.push_str(&top_n(&tool_counts, 8));
            out.push_str("\n\n");
        }

        if !command_counts.is_empty() {
            out.push_str("[자주 쓴 명령]\n");
            out.push_str(&top_n(&command_counts, 12));
            out.push_str("\n\n");
        }

        if !file_counts.is_empty() {
            out.push_str("[자주 건드린 파일]\n");
            out.push_str(&top_n(&file_counts, 10));
            out.push('\n');
        }

        out.trim_end().to_string()
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
            } else if path.is_file() && !Self::is_excluded(&path) {
                out.push(path);
            }
        }
    }
}

impl SessionConnector {
    /// Files still outside the two watermarks, i.e. what a later run would take.
    fn pending(&self, cursor: Option<Cursor>) -> Result<usize> {
        let (seen_newest, seen_oldest) = Self::parse_cursor(cursor);
        let mut files = Vec::new();
        for root in self.roots() {
            if root.exists() {
                Self::scan_dir(&root, &mut files);
            }
        }
        let mut count = 0usize;
        for path in files {
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            let modified: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            let fresh = seen_newest.is_none_or(|w| modified > w);
            let backfill = seen_oldest.is_some_and(|w| modified < w);
            if fresh || backfill {
                count += 1;
            }
        }
        Ok(count)
    }
}

#[async_trait]
impl Connector for SessionConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Session
    }

    async fn remaining(&self, cursor: Option<Cursor>) -> Result<usize> {
        self.pending(cursor)
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        let (seen_newest, seen_oldest) = Self::parse_cursor(cursor);

        let mut files = Vec::new();
        for root in self.roots() {
            if root.exists() {
                Self::scan_dir(&root, &mut files);
            }
        }

        let mut all: Vec<(PathBuf, DateTime<Utc>)> = Vec::new();
        for path in files {
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => return Err(AppError::Io(format!("stat {}: {e}", path.display()))),
            };
            let modified: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            all.push((path, modified));
        }
        // Newest first throughout: what the owner did most recently is what
        // they are most likely to ask their persona about.
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Anything newer than the forward watermark, then — with whatever
        // budget is left — keep walking backwards from the backfill frontier.
        let fresh = all
            .iter()
            .filter(|(_, m)| seen_newest.is_none_or(|w| *m > w));
        let backfill = all
            .iter()
            .filter(|(_, m)| seen_oldest.is_some_and(|w| *m < w));
        let selected: Vec<(PathBuf, DateTime<Utc>)> = fresh
            .chain(backfill)
            .take(MAX_FILES_PER_SYNC)
            .cloned()
            .collect();

        let mut items = Vec::new();
        let mut newest = seen_newest;
        let mut oldest = seen_oldest;

        for (path, modified) in selected {
            let raw = fs::read_to_string(&path)
                .map_err(|e| AppError::Io(format!("read {}: {e}", path.display())))?;
            let text = Self::tail_chars(&Self::digest_transcript(&raw), MAX_CHARS_PER_ITEM);

            // A transcript with no conversation in it (tooling-only session)
            // carries nothing to learn from, but its mtime still counts as
            // covered so the watermarks move past it.
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
            oldest = Some(match oldest {
                Some(o) if o <= modified => o,
                _ => modified,
            });
        }

        Ok((items, Self::render_cursor(newest, oldest)))
    }

    /// Re-scope collection to `config.roots`, or back to auto-detection when
    /// the list is absent or empty.
    ///
    /// Empty means "everything I can find", not "nothing": a scope picker with
    /// every box unchecked has to keep working, and silently collecting from
    /// no directories would look identical to a broken connector.
    fn configure(&self, config: &SourceConfig) -> Result<()> {
        let roots: Vec<PathBuf> = config
            .0
            .get("roots")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();
        let roots = if roots.is_empty() {
            Self::default_roots()
        } else {
            roots
        };
        *self.roots.write().unwrap_or_else(|e| e.into_inner()) = roots;
        Ok(())
    }

    fn supports_manual(&self) -> bool {
        true // "지금 수집" also runs the session sync (US-1.2).
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    /// `count` files with strictly increasing mtimes, newest last.
    fn transcripts(dir: &Path, count: usize) -> Vec<PathBuf> {
        let base = SystemTime::now() - Duration::from_secs(86_400 * (count as u64 + 1));
        (0..count)
            .map(|i| {
                let p = dir.join(format!("s{i:03}.jsonl"));
                fs::write(
                    &p,
                    format!(r#"{{"message":{{"role":"user","content":"세션 {i}"}}}}"#),
                )
                .unwrap();
                let t = base + Duration::from_secs(86_400 * i as u64);
                fs::File::options()
                    .write(true)
                    .open(&p)
                    .unwrap()
                    .set_modified(t)
                    .unwrap();
                p
            })
            .collect()
    }

    fn titles(items: &[RawItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| i.text.clone().unwrap_or_default().trim().to_string())
            .collect()
    }

    #[tokio::test]
    async fn first_run_takes_the_newest_sessions_not_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        transcripts(dir.path(), 5);
        let c = SessionConnector::new(vec![dir.path().to_path_buf()]);

        let (items, _) = c
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();

        // A months-old transcript directory must not open with material the
        // owner has long forgotten.
        assert!(titles(&items)[0].contains("세션 4"));
    }

    fn mtime_of(path: &Path) -> DateTime<Utc> {
        DateTime::<Utc>::from(fs::metadata(path).unwrap().modified().unwrap())
    }

    #[tokio::test]
    async fn later_runs_backfill_older_history_instead_of_repeating() {
        let dir = tempfile::tempdir().unwrap();
        let files = transcripts(dir.path(), 5); // oldest .. newest
        let c = SessionConnector::new(vec![dir.path().to_path_buf()]);

        // Stand in for a run that covered only the two newest files: the
        // forward watermark sits at the newest, the backfill frontier at the
        // second-newest.
        let cursor =
            SessionConnector::render_cursor(Some(mtime_of(&files[4])), Some(mtime_of(&files[3])));

        let (items, next) = c
            .sync(Some(cursor), &crate::core::traits::NoProgress)
            .await
            .unwrap();

        // Nothing is newer than the frontier, so the run must walk backwards
        // rather than hand back the same two files.
        assert_eq!(items.len(), 3, "the three older sessions get picked up");
        assert!(
            titles(&items)[0].contains("세션 2"),
            "newest-first within the backfill"
        );

        let (n2, o2) = SessionConnector::parse_cursor(Some(next));
        assert_eq!(
            n2.unwrap(),
            mtime_of(&files[4]),
            "forward watermark stays put"
        );
        assert_eq!(
            o2.unwrap(),
            mtime_of(&files[0]),
            "frontier moves to the oldest"
        );
    }

    #[tokio::test]
    async fn the_backlog_left_after_a_run_is_reported() {
        // Pressing collect on a long history keeps returning new items, which
        // is correct but reads as re-collection unless the remainder is shown.
        let dir = tempfile::tempdir().unwrap();
        let files = transcripts(dir.path(), 5);
        let c = SessionConnector::new(vec![dir.path().to_path_buf()]);

        let cursor =
            SessionConnector::render_cursor(Some(mtime_of(&files[4])), Some(mtime_of(&files[3])));
        assert_eq!(c.remaining(Some(cursor)).await.unwrap(), 3);

        let (_, drained) = c
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert_eq!(c.remaining(Some(drained)).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_drained_directory_yields_nothing_on_the_next_run() {
        let dir = tempfile::tempdir().unwrap();
        transcripts(dir.path(), 5);
        let c = SessionConnector::new(vec![dir.path().to_path_buf()]);

        let (first, cursor) = c
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert_eq!(first.len(), 5);

        let (again, _) = c
            .sync(Some(cursor), &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert!(again.is_empty(), "already-covered history must not repeat");
    }

    #[tokio::test]
    async fn subagent_output_is_not_the_owners_context() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("subagents").join("workflows");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("agent-1.jsonl"),
            r#"{"message":{"role":"user","content":"에이전트 부산물"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("mine.jsonl"),
            r#"{"message":{"role":"user","content":"내 세션"}}"#,
        )
        .unwrap();

        let c = SessionConnector::new(vec![dir.path().to_path_buf()]);
        let (items, _) = c
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert!(titles(&items)[0].contains("내 세션"));
    }

    #[test]
    fn a_legacy_single_timestamp_cursor_still_reads() {
        let (newest, oldest) =
            SessionConnector::parse_cursor(Some(Cursor("2026-01-01T00:00:00Z".into())));
        assert!(newest.is_some());
        assert!(oldest.is_none(), "legacy cursor has no backfill frontier");
    }
}

#[cfg(test)]
mod digest_tests {
    use super::*;

    const TRANSCRIPT: &str = r#"
{"type":"pr-link","prNumber":"2"}
{"message":{"role":"user","content":"배포 절차 알려줘"}}
{"message":{"role":"user","content":"<system-reminder>ignore me</system-reminder>"}}
{"message":{"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"AAAAAAAAAAAAAAAA"},{"type":"text","text":"조사해볼게요"},{"type":"tool_use","name":"Bash","input":{"command":"cd /Users/me/proj && cargo test --all"}},{"type":"tool_use","name":"Bash","input":{"command":"git status"}},{"type":"tool_use","name":"Edit","input":{"file_path":"/Users/me/proj/src/main.rs"}},{"type":"tool_use","name":"mcp__abc-123__notion-update-page","input":{}}]}}
{"message":{"role":"user","content":"고마워, 그대로 진행해줘"}}
"#;

    #[test]
    fn keeps_the_owners_words_and_drops_the_machinery() {
        let out = SessionConnector::digest_transcript(TRANSCRIPT);

        assert!(out.contains("배포 절차 알려줘"));
        assert!(out.contains("고마워, 그대로 진행해줘"));
        // The assistant's prose is not a fact about the owner.
        assert!(!out.contains("조사해볼게요"));
        // Neither is the harness boilerplate that arrives under the user role.
        assert!(!out.contains("ignore me"));
        // Nor the bytes that dominate a real transcript.
        assert!(!out.contains("signature"));
        assert!(!out.contains("AAAAAAAA"));
    }

    #[test]
    fn profiles_tools_commands_and_files() {
        let out = SessionConnector::digest_transcript(TRANSCRIPT);

        assert!(out.contains("Bash (2)"), "tool counts: {out}");
        assert!(out.contains("Edit (1)"));
        // An MCP id is unreadable in full; the tool name is the useful part.
        assert!(out.contains("mcp:notion-update-page"));
        // `cd X && cargo test` characterises the owner as a cargo user, not a cd user.
        assert!(out.contains("cargo test"), "commands: {out}");
        assert!(out.contains("git status"));
        assert!(out.contains("/Users/me/proj"), "project: {out}");
    }

    #[test]
    fn a_digest_is_orders_of_magnitude_smaller_than_the_transcript() {
        let out = SessionConnector::digest_transcript(TRANSCRIPT);
        assert!(
            out.len() < TRANSCRIPT.len() / 2,
            "digest {} vs transcript {}",
            out.len(),
            TRANSCRIPT.len()
        );
    }

    #[test]
    fn plain_text_logs_pass_through_unchanged() {
        let raw = "이건 JSONL이 아니라 그냥 텍스트 로그입니다.\n두 번째 줄.";
        assert_eq!(SessionConnector::digest_transcript(raw), raw);
    }

    #[test]
    fn a_tooling_only_transcript_digests_to_nothing() {
        let raw = r#"{"type":"queue-operation","op":"x"}"#;
        assert!(SessionConnector::digest_transcript(raw).trim().is_empty());
    }

    #[test]
    fn command_labels_name_the_program_not_the_invocation() {
        assert_eq!(command_label("git status 2>&1 || echo x"), "git status");
        assert_eq!(command_label("cd /a/b && npm run dev"), "npm run");
        assert_eq!(command_label("/usr/local/bin/rg foo"), "rg");
        assert_eq!(command_label("FOO=1 cargo build"), "cargo build");
        assert_eq!(command_label("ls -la /tmp"), "ls");
    }

    #[test]
    fn tail_is_kept_and_never_splits_a_character() {
        // Korean is multi-byte: a byte-wise tail would panic or produce mojibake.
        let text: String = "가나다라마바사아자차".chars().cycle().take(100).collect();
        let tail = SessionConnector::tail_chars(&text, 10);

        assert_eq!(tail.chars().count(), 10);
        assert!(text.ends_with(&tail));
        assert_eq!(SessionConnector::tail_chars("짧음", 10), "짧음");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn the_label_comes_from_the_transcript_not_the_directory_name() {
        // The bug this replaces: the directory name is the working directory
        // with `/`, `_` and `.` all flattened to `-`, so decoding it produced
        // "Work/18/avatar/knows/me" for "Work/18_avatar/knows-me".
        let root = tmp_root().join("label1");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut fh = fs::File::create(root.join("s.jsonl")).unwrap();
        writeln!(
            fh,
            r#"{{"type":"user","cwd":"/Users/x/Desktop/Work/18_avatar/knows-me"}}"#
        )
        .unwrap();

        let label = project_label(&root, "-Users-x-Desktop-Work-18-avatar-knows-me");
        assert_eq!(
            label, "/Users/x/Desktop/Work/18_avatar/knows-me",
            "underscores and dashes must survive"
        );
    }

    #[test]
    fn a_transcript_without_cwd_falls_back_to_the_directory_name() {
        let root = tmp_root().join("label2");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut fh = fs::File::create(root.join("s.jsonl")).unwrap();
        writeln!(fh, r#"{{"type":"user","message":{{"content":"hi"}}}}"#).unwrap();

        // Imperfect, but it is a name rather than an empty row.
        assert_eq!(project_label(&root, "-Users-x-Work"), "Users/x/Work");
    }

    #[test]
    fn the_home_prefix_is_trimmed_but_a_foreign_path_is_kept_whole() {
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            assert_eq!(
                shorten_path(&format!("{home}/Desktop/Work")),
                "Desktop/Work"
            );
        }
        assert_eq!(shorten_path("/opt/elsewhere/proj"), "/opt/elsewhere/proj");
    }

    #[test]
    fn everything_is_kept_when_it_fits() {
        let p: Vec<String> = ["왜 이걸 하냐면", "그래서 이렇게 했다", "결론"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (kept, dropped) = select_prompts(&p, 18_000);
        assert_eq!(dropped, 0);
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn the_opening_survives_a_budget_squeeze() {
        // The regression this replaced: a tail-only window kept the last turns
        // and dropped the first, which is where the owner says *why* they are
        // doing something — the one thing the knowledge base exists to hold.
        let mut p: Vec<String> = vec!["왜 이걸 하냐면 배포가 계속 실패해서다".into()];
        p.extend((0..200).map(|i| format!("중간 반복 {i}")));
        p.push("결론: main 직접 푸시 금지".into());

        let (kept, dropped) = select_prompts(&p, 400);
        let text: Vec<&str> = kept.iter().map(|s| s.as_str()).collect();

        assert!(dropped > 0, "a 202-turn session must not fit in 400 chars");
        assert!(
            text.first().is_some_and(|t| t.starts_with("왜 이걸")),
            "the opening turn must survive: {text:?}"
        );
        assert!(
            text.last().is_some_and(|t| t.starts_with("결론")),
            "the closing turn must survive: {text:?}"
        );
    }

    #[test]
    fn kept_turns_stay_in_the_order_they_were_said() {
        // Taking from both ends must not hand the model a shuffled transcript.
        let p: Vec<String> = (0..40).map(|i| format!("{i:02} 발화")).collect();
        let (kept, _) = select_prompts(&p, 200);
        let nums: Vec<&str> = kept.iter().map(|s| &s.as_str()[..2]).collect();
        let mut sorted = nums.clone();
        sorted.sort_unstable();
        assert_eq!(nums, sorted, "order was not preserved: {nums:?}");
    }

    #[test]
    fn a_budget_too_small_for_any_turn_drops_everything_rather_than_panicking() {
        let p: Vec<String> = vec!["아주 긴 발화".repeat(20)];
        let (kept, dropped) = select_prompts(&p, 1);
        assert!(kept.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn the_apps_own_extraction_prompts_are_not_owner_speech() {
        // The CLI backend's request is recorded as a user turn in Claude Code's
        // transcript, in the very directory this connector scans. Left in, each
        // extraction run teaches the next one about knows-me's own prompts.
        let mut out = Vec::new();
        for prompt in [
            crate::llm::prompts::SUMMARIZE_SYSTEM,
            crate::llm::prompts::CLASSIFY_SYSTEM,
        ] {
            // Exactly what `ClaudeCliLlm::run` writes to stdin.
            push_prompt(&mut out, &format!("{prompt}\n\n어떤 세션 본문"));
        }
        assert!(
            out.is_empty(),
            "the app's own prompts must not be collected as the owner talking: {out:?}"
        );
    }

    #[test]
    fn a_real_turn_that_merely_mentions_extraction_is_kept() {
        // The filter keys on the prompts' opening line, not on the topic —
        // the owner discussing extraction is exactly the context worth having.
        let mut out = Vec::new();
        push_prompt(&mut out, "추출 품질이 별로야, 분류 스키마를 다시 보자");
        assert_eq!(out.len(), 1);
    }

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
        let (items, cursor) = conn
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, SourceKind::Session);
        assert!(items[0].text.as_deref().unwrap().contains("hello session"));
        assert!(!cursor.0.is_empty());

        // Re-sync with the returned cursor → nothing new (incremental).
        let (again, _) = conn
            .sync(Some(cursor), &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert_eq!(again.len(), 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn missing_root_is_empty_not_error() {
        let conn = SessionConnector::new(vec![PathBuf::from("/nonexistent/km/path")]);
        let (items, _) = conn
            .sync(None, &crate::core::traits::NoProgress)
            .await
            .unwrap();
        assert_eq!(items.len(), 0);
    }
}
