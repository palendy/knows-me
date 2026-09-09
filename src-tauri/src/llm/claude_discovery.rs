//! Detect local Claude Code CLI installs so the owner can pick which one the
//! `claude-cli` backend drives — the one on this OS's PATH, plus (on Windows)
//! the `claude` inside each installed WSL distro. The two often differ in login
//! state: someone may be logged into the WSL CLI but not the Windows one, so the
//! app must be able to point at whichever is actually usable.

use std::process::Command;

use serde::Serialize;

/// One discovered Claude Code install, as offered to the settings picker.
#[derive(Serialize, Clone, Debug)]
pub struct ClaudeInstall {
    /// Stable id, e.g. `"native"` or `"wsl:Ubuntu"`.
    pub id: String,
    /// Human label for the picker (e.g. "Windows", "WSL · Ubuntu").
    pub label: String,
    /// The command line the backend invokes — stored into
    /// [`AppConfig::llm_binary`](crate::core::types::AppConfig). Either `claude`
    /// (on PATH) or `wsl -d <distro> claude`.
    pub binary: String,
    /// Model this install currently has configured (`~/.claude/settings.json`),
    /// if readable — used to prefill the model field. `None` when unknown.
    pub model: Option<String>,
}

/// The `model` field of a `~/.claude/settings.json` document, if present.
fn parse_model(settings_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(settings_json)
        .ok()?
        .get("model")?
        .as_str()
        .map(str::to_string)
}

/// Discover every Claude Code CLI reachable from here. Best-effort: anything
/// that errors or isn't installed is simply omitted.
pub fn discover() -> Vec<ClaudeInstall> {
    let mut installs = Vec::new();

    // 1. The `claude` on this OS's PATH (the one the backend uses by default).
    if native_has_claude() {
        installs.push(ClaudeInstall {
            id: "native".into(),
            label: native_label(),
            binary: "claude".into(),
            model: native_settings_model(),
        });
    }

    // 2. On Windows, each WSL distro's `claude`, invoked via `wsl -d <distro>`
    //    with the *absolute* path — a non-login `wsl` shell doesn't have
    //    ~/.local/bin on PATH, so a bare `claude` would be "command not found".
    #[cfg(windows)]
    for distro in wsl_distros() {
        if let Some((path, model)) = wsl_claude(&distro) {
            installs.push(ClaudeInstall {
                id: format!("wsl:{distro}"),
                label: format!("WSL · {distro}"),
                binary: format!("wsl -d {distro} {path}"),
                model,
            });
        }
    }

    installs
}

#[cfg(windows)]
fn native_label() -> String {
    "Windows".into()
}
#[cfg(not(windows))]
fn native_label() -> String {
    "로컬".into()
}

/// Don't pop a console when spawning a child on Windows.
#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

/// Is there a `claude` on this OS's PATH? (`where` on Windows, `which` elsewhere.)
fn native_has_claude() -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    let mut cmd = Command::new(finder);
    cmd.arg("claude");
    no_window(&mut cmd);
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

/// Read the native `~/.claude/settings.json` `model`, if present.
fn native_settings_model() -> Option<String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)?;
    let text = std::fs::read_to_string(home.join(".claude/settings.json")).ok()?;
    parse_model(&text)
}

/// Installed WSL distros (`wsl -l -q`, UTF-16LE, one per line).
#[cfg(windows)]
fn wsl_distros() -> Vec<String> {
    let mut cmd = Command::new("wsl.exe");
    cmd.args(["--list", "--quiet"]);
    no_window(&mut cmd);
    let stdout = match cmd.output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let utf16: Vec<u16> = stdout
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16_lossy(&utf16)
        .lines()
        .map(|l| {
            l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// If `<distro>` has a `claude`, return `(absolute_path, configured_model)`;
/// `None` when the distro has no claude. The path comes from a *login* shell
/// (`sh -lc`, which has ~/.local/bin on PATH) so we can invoke it later without
/// one — the native install is a self-contained binary.
#[cfg(windows)]
fn wsl_claude(distro: &str) -> Option<(String, Option<String>)> {
    let mut probe = Command::new("wsl.exe");
    probe.args(["-d", distro, "--", "sh", "-lc", "command -v claude"]);
    no_window(&mut probe);
    let out = probe.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    // Best-effort model read from the distro's own settings.json.
    let mut cat = Command::new("wsl.exe");
    cat.args([
        "-d",
        distro,
        "--",
        "sh",
        "-lc",
        "cat ~/.claude/settings.json",
    ]);
    no_window(&mut cat);
    let model = cat
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_model(&String::from_utf8_lossy(&o.stdout)));
    Some((path, model))
}
