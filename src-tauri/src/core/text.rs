//! Text similarity for suppressing things we already know.
//!
//! The interview queue kept filling with the same idea written five different
//! ways. The suppression that was supposed to prevent it compared titles for
//! exact equality after `trim().to_lowercase()`, and what actually arrives is
//! *paraphrase*, not repetition — the summarizer reads each session on its own
//! and writes a fresh sentence every time. Measured on 30 titles the extractor
//! produced from six real sessions: 13 distinct ideas, 57% redundancy, and
//! **zero** byte-identical pairs. Exact matching had a 0% hit rate.
//!
//! These helpers are pure so the threshold can be justified with numbers rather
//! than taste, and so the same rule can be applied from both the queue (U3) and
//! the processing pipeline (U2) without either importing the other.

/// Similarity at or above which two titles are treated as the same item.
///
/// Chosen against the 30 real titles above. Every pair at or above this scored
/// as a genuine duplicate, and the closest unrelated pair sat far below it:
///
/// ```text
/// 0.76  기술 작업은 AI에 맡기고 본인은 학습에만 집중하려 함
///       기술 작업은 AI에 맡기고 본인은 공부에만 집중하려 함
/// 0.56  세션 데이터를 모아도 쓸 만한 정보가 안 나오는 문제
///       세션 로그를 쌓아도 쓸 만한 정보가 안 나오는 문제
/// ```
///
/// Deliberately conservative. Lexical overlap cannot see that "'아바타 카드'의
/// 정의가 아직 확정되지 않음" and "아바타 카드가 정의되지 않아 답변이 두 번
/// 막힘" are one fact — they share 0.26. Lowering the bar far enough to catch
/// those would start merging unrelated titles, which is the worse failure: a
/// duplicate question is noise, a wrongly-merged one is lost knowledge. The
/// cure for the semantic cases is upstream — showing the summarizer the titles
/// already recorded so it reuses one instead of inventing a synonym.
pub const NEAR_DUPLICATE: f32 = 0.45;

/// Fold a title to its comparable form: lowercase, no decoration, no spaces.
///
/// Quotes carry no meaning here and the model applies them inconsistently —
/// `'아바타'`, `"아바타"` and `아바타` are one word. Spaces go too, because
/// Korean tokenizes at the character level for our purposes and spacing varies
/// freely between paraphrases of the same sentence.
pub fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_whitespace() && (c.is_alphanumeric() || *c == '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Jaccard overlap of character bigrams, in `0.0..=1.0`.
///
/// Character bigrams rather than words: Korean paraphrase changes particles and
/// endings ("맡기고" / "위임하고", "학습에만" / "공부에만") which destroys word
/// overlap while leaving most of the sentence intact.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (bigrams(&normalize(a)), bigrams(&normalize(b)));
    if a.is_empty() && b.is_empty() {
        // Two titles with nothing comparable in them (punctuation only) are not
        // evidence of sameness. Saying 0 keeps them separate; a caller that
        // wants exact equality can compare `normalize` directly.
        return 0.0;
    }
    let inter = a.intersection(&b).count() as f32;
    let union = a.union(&b).count() as f32;
    inter / union
}

/// Whether two titles say the same thing closely enough to suppress one.
pub fn is_near_duplicate(a: &str, b: &str) -> bool {
    let (na, nb) = (normalize(a), normalize(b));
    // Exact-after-normalization is a duplicate even when it is too short to
    // produce a bigram ("A" vs "a"), which `similarity` alone would score 0.
    if !na.is_empty() && na == nb {
        return true;
    }
    similarity(a, b) >= NEAR_DUPLICATE
}

fn bigrams(s: &str) -> std::collections::HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn decoration_and_spacing_do_not_make_a_new_title() {
        // The model quotes the same term differently from one session to the
        // next; that is not a different fact.
        assert!(is_near_duplicate("'아바타' 카드", "\"아바타\"카드"));
        assert!(is_near_duplicate("**배포 절차**", "배포절차"));
    }

    #[test]
    fn paraphrase_of_the_same_sentence_is_caught() {
        // Real pair from the extractor, scored 0.76.
        assert!(is_near_duplicate(
            "기술 작업은 AI에 맡기고 본인은 학습에만 집중하려 함",
            "기술 작업은 AI에 맡기고 본인은 공부에만 집중하려 함",
        ));
        // Real pair, scored 0.56.
        assert!(is_near_duplicate(
            "세션 데이터를 모아도 쓸 만한 정보가 안 나오는 문제",
            "세션 로그를 쌓아도 쓸 만한 정보가 안 나오는 문제",
        ));
    }

    #[test]
    fn unrelated_titles_are_not_merged() {
        // Losing a distinct fact is worse than asking one question twice, so
        // the bar has to stay above anything that merely shares vocabulary.
        assert!(!is_near_duplicate(
            "PDF 한글화에서 Mermaid 다이어그램 글자가 안 보이는 문제",
            "세션 로그를 쌓아도 쓸 만한 정보가 안 나오는 문제",
        ));
        assert!(!is_near_duplicate("배포 절차", "회의록 정리 규칙"));
    }

    #[test]
    fn a_semantic_duplicate_lexical_overlap_cannot_see_is_documented_as_missed() {
        // Not an aspiration — a boundary. These two are one fact and this layer
        // does not catch them; the summarizer hint upstream is what must.
        // Pinning it keeps a later threshold change from being made blind.
        let a = "'아바타 카드'의 정의가 아직 확정되지 않음";
        let b = "아바타 카드가 정의되지 않아 답변이 두 번 막힘";
        assert!(
            similarity(a, b) < NEAR_DUPLICATE,
            "got {}",
            similarity(a, b)
        );
    }

    #[test]
    fn empty_and_punctuation_only_titles_never_match() {
        assert!(!is_near_duplicate("", ""));
        assert!(!is_near_duplicate("...", "---"));
    }

    proptest! {
        #[test]
        fn a_title_is_always_a_duplicate_of_itself(s in "\\PC{1,80}") {
            prop_assume!(!normalize(&s).is_empty());
            prop_assert!(is_near_duplicate(&s, &s));
        }

        #[test]
        fn similarity_is_symmetric(a in "\\PC{0,60}", b in "\\PC{0,60}") {
            prop_assert_eq!(similarity(&a, &b), similarity(&b, &a));
        }

        #[test]
        fn similarity_stays_in_range(a in "\\PC{0,60}", b in "\\PC{0,60}") {
            let s = similarity(&a, &b);
            prop_assert!((0.0..=1.0).contains(&s), "got {}", s);
        }
    }
}
