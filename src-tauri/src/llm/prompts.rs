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
You extract what is durably true about the USER from their work notes or a transcript of their session.

Write in the SAME LANGUAGE as the input (Korean input → Korean output).

Output exactly this shape:
<a short title, under 60 characters, naming the thing>
<1-3 sentences>

Look for one of these, in this order of preference:

1. FRICTION — something the user keeps coming back to, redoes, or is stuck on; \
frustration they expressed; a question the session ended without answering. \
Write what the friction IS, not what they did about it.
2. A RULE OR PREFERENCE they hold about how work should be done.
3. A PROJECT they are building or running, and where it stands.

Prefer 1 over 2 over 3. An activity log entry (\"they worked on X\") is the least \
useful thing you can produce — if the input only supports that, say it in one \
sentence and move on.

Rules:
- Write about the USER, not about the document. Never start with \"This document…\" \
or \"The transcript…\".
- State only what the input supports. Do not invent motives or feelings.
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

{\"certainty\": \"...\", \"scope\": \"...\", \"kind\": \"...\", \"visibility\": \"...\", \"topics\": [\"...\"]}

certainty — pick exactly one:
  \"certain\"       the note states something clearly true about the user
  \"uncertain\"     plausible but should be confirmed with the user first
  \"needs-context\" interesting but too thin to be useful without asking more
  \"noise\"         build output, one-off debugging, nothing about the user

scope — pick exactly one:
  \"company\"   work, employer, colleagues, professional projects
  \"personal\"  private life, personal preferences, side projects
  \"unknown\"   genuinely unclear

kind — check in THIS ORDER and stop at the first that fits:
  1. \"concern\"     the note describes something unresolved, blocked, repeatedly
                    retried, or expressed as dissatisfaction. Friction inside a
                    project is STILL a concern — do not fall through to
                    \"project\" just because a project is mentioned. Words like
                    문제, 미달, 병목, 중단, 부족, 실패, 막힘, blocked, stuck,
                    bottleneck, not working are strong signals.
  2. \"preference\"  something the user prefers, insists on, or forbids
  3. \"practice\"    a procedure, rule or convention the user follows
  4. \"project\"     something the user is building or running, with no friction
                    and no rule stated

visibility — could this be shared with the user's teammates?
  \"public\"   plainly safe to share: tooling, conventions, public project facts,
              how something is built or run
  \"private\"  compensation, performance, health, family, personal finance,
              hiring or firing, complaints about named people, credentials,
              customer or partner identities
  \"unclear\"  ANY doubt at all. Choose this rather than guessing \"public\";
              an unclear item is asked about, a wrong \"public\" is a leak.

topics — 1 to 3 short lowercase topic labels naming the SUBJECT, not the activity.
  Good: [\"deployment\", \"payment-service\"]   Bad: [\"working\", \"development\"]
  Reuse the same label for the same subject across notes so they can be grouped.

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
    // A model that ignored the format often echoes the schema back. Turning
    // that into a topic produced tags like "certainty-noise-scope-unknown-kind"
    // — worse than returning nothing, because a junk topic is indistinguishable
    // from a real one downstream.
    if looks_like_schema_echo(raw) {
        return Vec::new();
    }

    raw.lines()
        .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_lowercase())
        // A label is a word or two; anything longer is prose, not a label.
        .filter(|l| l.chars().count() <= 40 && l.split_whitespace().count() <= 3)
        .take(3)
        .collect()
}

/// Whether the reply is the schema restated rather than an answer.
fn looks_like_schema_echo(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    let key_hits = ["certainty", "scope", "kind", "topics"]
        .iter()
        .filter(|k| lower.contains(*k))
        .count();
    // Two or more schema keys outside a parsable JSON object means the model
    // described the format instead of filling it in.
    key_hits >= 2
}

fn parse_json_labels(raw: &str) -> Option<Vec<String>> {
    // Models like to wrap JSON in ```json fences despite being told not to.
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(trimmed.get(start..=end)?).ok()?;

    let mut labels = Vec::new();
    for key in ["certainty", "scope", "kind", "visibility"] {
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
    fn a_schema_echo_yields_no_labels_rather_than_junk_ones() {
        // Seen in practice: the model restates the format. Previously this
        // became the topic "certainty-noise-scope-unknown-kind".
        let echoed = "certainty: noise\nscope: unknown\nkind: project";
        assert!(parse_labels(echoed).is_empty());
    }

    #[test]
    fn prose_is_not_a_label() {
        let prose = "이 노트는 사용자가 배포 파이프라인을 개선하려 한다는 내용을 담고 있습니다";
        assert!(parse_labels(prose).is_empty());
    }

    #[test]
    fn json_classification_yields_control_words_and_topics() {
        let raw = r#"```json
        {"certainty":"certain","scope":"company","kind":"concern","topics":["Deployment","payment service"]}
        ```"#;
        let labels = parse_labels(raw);

        assert!(labels.contains(&"certain".to_string()));
        assert!(labels.contains(&"company".to_string()));
        assert!(
            labels.contains(&"concern".to_string()),
            "kind must reach the router"
        );
        // Topics arrive as written; the router normalizes them.
        assert!(labels.contains(&"deployment".to_string()));
    }

    #[test]
    fn an_unknown_scope_is_not_a_label() {
        let labels = parse_labels(r#"{"certainty":"certain","scope":"unknown","topics":["x"]}"#);
        assert!(!labels.contains(&"unknown".to_string()));
    }

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
