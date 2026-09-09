//! Prompt construction for the cloud LLM gateway. Pure and testable so the
//! network client (feature `llm-http`) stays a thin transport shell.
//!
//! All callers pass **already-masked** text (US-2.2); these prompts never
//! reintroduce raw identifiers.

/// System prompt for extracting a durable fact about the owner.
///
/// The job is *not* to summarize the document. A summary of a transcript
/// ("the document describes a trading bot project") is worthless as personal
/// context — the knowledge base is meant to answer "what is true about me",
/// so the extraction has to be about the person: what they decided, prefer,
/// use, are building, or are constrained by.
///
/// Output language follows the input, because a Korean owner asking their
/// persona a question in Korean should not get context stored in English.
pub const SUMMARIZE_SYSTEM: &str = "\
You extract durable facts about the USER from their work notes or a transcript of their session.

Write in the SAME LANGUAGE as the input (Korean input → Korean output).

Output exactly this shape:
<a short title, under 60 characters, naming the fact>
<1-3 sentences stating what is true about the user: their decision, preference, \
tool, project, constraint, or working style — and why, if the input says>

Rules:
- Write about the USER, not about the document. Never start with \"This document…\" \
or \"The transcript…\".
- State only what the input supports. Do not infer personality or motives.
- Prefer what will still be true next month over what happened once.
- If the input contains nothing durable about the user (pure tooling output, \
build logs, one-off debugging), output exactly: NOTHING
- Identifiers are masked as tokens like «EMAIL_1»; keep them verbatim.
- Output only the title and body. No preamble, no markdown fences, no labels.";

/// System prompt for classifying a masked item.
///
/// The vocabulary here is not decorative: [`crate::processing::route`] switches
/// on `certainty` and `scope` to decide store-vs-ask-vs-drop. An earlier
/// version asked for free-form topic labels only, so the router never saw a
/// control label, every item fell through to "store", and the interview queue
/// could never fill — the feature existed but was unreachable.
pub const CLASSIFY_SYSTEM: &str = "\
You classify a note about a user. Reply with ONLY a JSON object, no fences:

{\"certainty\": \"...\", \"scope\": \"...\", \"topics\": [\"...\"]}

certainty — pick exactly one:
  \"certain\"       the note states something clearly true about the user
  \"uncertain\"     plausible but should be confirmed with the user first
  \"needs-context\" interesting but too thin to be useful without asking more
  \"noise\"         build output, one-off debugging, nothing about the user

scope — pick exactly one:
  \"company\"   work, employer, colleagues, professional projects
  \"personal\"  private life, personal preferences, side projects
  \"unknown\"   genuinely unclear

topics — 1 to 3 short lowercase topic labels, e.g. [\"deployment\", \"tooling\"]

Masked tokens like «EMAIL_1» may appear; ignore them when classifying.";

/// System prompt for the owner-facing persona chat.
pub const CHAT_SYSTEM: &str = "You are the user's personal context assistant. Answer using the provided \
context. Masked tokens like «EMAIL_1» stand in for the user's private identifiers; treat them as opaque \
references. Be concise and grounded; if the context does not cover something, say so.";

/// Marker the summarizer emits when an item holds nothing durable.
pub const NOTHING_MARKER: &str = "NOTHING";

/// Whether a summary says "there is no fact here".
pub fn is_nothing(summary: &str) -> bool {
    summary.trim().eq_ignore_ascii_case(NOTHING_MARKER)
}

/// Parse the model's classification response into router labels.
///
/// Accepts the JSON object [`CLASSIFY_SYSTEM`] asks for, and falls back to the
/// older one-label-per-line form so a model that ignores the format still
/// yields usable topics rather than nothing.
pub fn parse_labels(raw: &str) -> Vec<String> {
    if let Some(labels) = parse_json_labels(raw) {
        return labels;
    }
    raw.lines()
        .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_lowercase())
        .take(3)
        .collect()
}

fn parse_json_labels(raw: &str) -> Option<Vec<String>> {
    // Models like to wrap JSON in ```json fences despite being told not to.
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(trimmed.get(start..=end)?).ok()?;

    let mut labels = Vec::new();
    for key in ["certainty", "scope"] {
        if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
            let v = v.trim().to_lowercase();
            // "unknown" is the absence of a scope, not a label to match on.
            if !v.is_empty() && v != "unknown" {
                labels.push(v);
            }
        }
    }
    if let Some(topics) = value.get("topics").and_then(|v| v.as_array()) {
        for t in topics.iter().take(3) {
            if let Some(t) = t.as_str() {
                let t = t.trim().to_lowercase();
                if !t.is_empty() {
                    labels.push(t);
                }
            }
        }
    }
    (!labels.is_empty()).then_some(labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bulleted_labels() {
        let out = parse_labels("- Deployment\n* contacts\n\nPreferences\n");
        assert_eq!(out, vec!["deployment", "contacts", "preferences"]);
    }

    #[test]
    fn caps_at_three_labels() {
        assert_eq!(parse_labels("a\nb\nc\nd\ne").len(), 3);
    }

    #[test]
    fn empty_response_yields_no_labels() {
        assert!(parse_labels("   \n\n").is_empty());
    }
}
