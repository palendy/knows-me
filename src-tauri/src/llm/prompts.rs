//! Prompt construction for the cloud LLM gateway. Pure and testable so the
//! network client (feature `llm-http`) stays a thin transport shell.
//!
//! All callers pass **already-masked** text (US-2.2); these prompts never
//! reintroduce raw identifiers.

/// System prompt for turning a masked raw item into a concise fact summary.
pub const SUMMARIZE_SYSTEM: &str =
    "You summarize a user's work/context notes into a single concise, \
factual sentence or two. The input has had identifiers masked as tokens like «EMAIL_1»; keep those \
tokens verbatim. Output only the summary, no preamble.";

/// System prompt for classifying a masked item into short topic labels.
pub const CLASSIFY_SYSTEM: &str = "You classify a user's context note into 1-3 short lowercase topic \
labels (e.g. \"deployment\", \"contacts\", \"preferences\"). Masked tokens like «EMAIL_1» may appear; \
ignore them for labeling. Output only the labels, one per line, no other text.";

/// System prompt for the owner-facing persona chat.
pub const CHAT_SYSTEM: &str = "You are the user's personal context assistant. Answer using the provided \
context. Masked tokens like «EMAIL_1» stand in for the user's private identifiers; treat them as opaque \
references. Be concise and grounded; if the context does not cover something, say so.";

/// Parse the model's classification response (one label per line) into labels.
pub fn parse_labels(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_lowercase())
        .take(3)
        .collect()
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
