//! Question intent — what kind of answer the owner is asking for.
//!
//! Keyword retrieval answers "무엇에 대해 아는가". It cannot answer "무엇을
//! 걱정하는가", because a worry is not a word that appears in the text; it is a
//! *shape* across many observations — the same subject returned to repeatedly,
//! or returned to without resolution.
//!
//! So a question of that shape must retrieve differently: over topic pages
//! ranked by repetition and recency, not over individual facts matched by term.
//! Detection is a cue-word heuristic. It is deliberately conservative — when
//! nothing matches, retrieval falls back to the keyword path, which is correct
//! for the great majority of questions.

/// What the owner is asking for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    /// "요즘 뭐가 걸려?" — subjects with unresolved friction.
    Concerns,
    /// "내가 뭐에 관심 있어?" — subjects that recur.
    Interests,
    /// "요즘 뭐 했어?" — subjects touched recently.
    Recent,
}

/// Cue words per intent. Korean first (the owner's language), then English.
///
/// Matching is substring-based on the lowercased prompt because Korean attaches
/// particles: "걱정" must match "걱정하는", "걱정이", "걱정돼" without a
/// morphological analyzer.
const CONCERN_CUES: &[&str] = &[
    "걱정",
    "고민",
    "막히",
    "막혀",
    "안 풀",
    "안풀",
    "어려움",
    "문제가",
    "골치",
    "신경 쓰",
    "신경쓰",
    "스트레스",
    "리스크",
    "불안",
    "worry",
    "worried",
    "concern",
    "stuck",
    "blocked",
    "pain point",
    "risk",
];

const INTEREST_CUES: &[&str] = &[
    "관심",
    "중요하게",
    "많이 보",
    "자주 보",
    "자주 하",
    "몰두",
    "파고",
    "interested",
    "care about",
    "focus on",
    "priorit",
];

const RECENT_CUES: &[&str] = &[
    "요즘",
    "최근",
    "요새",
    "근래",
    "지금 뭐",
    "요즘 뭐",
    "recently",
    "lately",
    "these days",
    "right now",
];

/// A question about the owner rather than about a subject.
///
/// Without this guard "배포가 걱정이야" — a statement *about deployment* — would
/// be routed to the concerns view and answered with a list of unrelated topics.
const SELF_CUES: &[&str] = &[
    "내가", "나는", "나의", "내 ", "제가", "저는", "우리", "my ", "i ", "am i", "do i",
];

fn has_cue(prompt: &str, cues: &[&str]) -> bool {
    cues.iter().any(|c| prompt.contains(c))
}

/// Classify the prompt, or `None` to use ordinary keyword retrieval.
pub fn detect(prompt: &str) -> Option<Intent> {
    let p = prompt.to_lowercase();

    // Concerns and interests are claims about the owner; require a self-cue so a
    // question about a subject stays a subject question.
    let about_self = has_cue(&p, SELF_CUES) || has_cue(&p, RECENT_CUES);

    if about_self && has_cue(&p, CONCERN_CUES) {
        return Some(Intent::Concerns);
    }
    if about_self && has_cue(&p, INTEREST_CUES) {
        return Some(Intent::Interests);
    }
    // "요즘 뭐 해?" — recency alone, but only for an open question rather than a
    // question that already names its subject.
    if has_cue(&p, RECENT_CUES) && is_open_question(&p) {
        return Some(Intent::Recent);
    }
    None
}

/// Whether the question is open ("뭐", "무엇", "what") rather than pointed at
/// something the owner already named.
fn is_open_question(prompt: &str) -> bool {
    const OPEN: &[&str] = &["뭐", "무엇", "어떤", "what", "which"];
    OPEN.iter().any(|w| prompt.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worries_are_recognized_through_korean_particles() {
        for q in [
            "내가 요즘 뭘 걱정하고 있지?",
            "나 요즘 뭐가 막혀 있어?",
            "요즘 내 고민이 뭐야",
            "what am i worried about lately",
        ] {
            assert_eq!(detect(q), Some(Intent::Concerns), "{q}");
        }
    }

    #[test]
    fn interests_are_separated_from_worries() {
        assert_eq!(
            detect("내가 요즘 관심 있는 게 뭐야?"),
            Some(Intent::Interests)
        );
    }

    #[test]
    fn an_open_recency_question_asks_for_recent_subjects() {
        assert_eq!(detect("요즘 뭐 하고 있었지?"), Some(Intent::Recent));
    }

    #[test]
    fn a_question_about_a_subject_stays_keyword_retrieval() {
        // These name a subject; answering them from a topic ranking would return
        // a list of unrelated topics instead of the answer.
        for q in [
            "배포 절차 알려줘",
            "결제 서비스 어떻게 띄워?",
            "이 리포 배포 규칙 있어?",
        ] {
            assert_eq!(detect(q), None, "{q}");
        }
    }

    #[test]
    fn a_statement_of_worry_about_a_subject_is_not_a_self_question() {
        // "배포가 걱정이야" is about deployment, not a request for a worry list.
        assert_eq!(detect("배포가 걱정이야"), None);
    }
}
