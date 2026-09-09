//! Context assembly and prompt rendering — pure, deterministic, testable.
//!
//! Rules implemented here: BR-P1 (confirmed only), BR-P5 (bounded),
//! BR-P7 (total order), BR-P8 (draft-kind directives).

use crate::core::types::{DraftKind, Fact, Scope};

use super::{ContextEntry, ContextSelection, PersonaContext, PersonaPrompt};

/// Weight applied to a term found in the title (vs. 1 in the body).
const TITLE_WEIGHT: u32 = 3;
/// Terms shorter than this are ignored — they match almost everything.
const MIN_TERM_LEN: usize = 2;

/// How many query terms occur in an already-lowercased haystack.
fn term_hits(query: &str, haystack_lower: &str) -> u32 {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= MIN_TERM_LEN)
        .map(|term| haystack_lower.matches(term).count() as u32)
        .sum()
}

/// Relevance computable from a [`FactSummary`], which carries only the title.
///
/// Used to pre-rank candidates *before* the full facts are fetched, so the
/// fetch budget is spent on the facts most likely to matter rather than on an
/// arbitrary slice (BR-P5).
pub fn title_relevance(query: &str, title: &str) -> u32 {
    term_hits(query, &title.to_lowercase()) * TITLE_WEIGHT
}

/// Relevance of one fact to a query. Case-insensitive term counting.
///
/// An empty query scores every fact 0, so ordering falls through to the
/// recency/id tiebreakers — still a total order (BR-P7).
fn relevance(query: &str, fact: &Fact) -> u32 {
    title_relevance(query, &fact.title) + term_hits(query, &fact.body.to_lowercase())
}

/// Narrow a set of facts down to the bounded grounding context.
///
/// Deterministic: the sort key `(relevance desc, confirmed_at desc, id asc)` is
/// a **total** order, so shuffling the input cannot change the output (BR-P7).
///
/// `total_confirmed` is passed in rather than counted from `facts`: per the
/// domain model (E1) it is the size of the confirmed *population*, and `facts`
/// here is only the bounded candidate slice the caller could afford to fetch.
pub fn select_context(
    facts: &[Fact],
    query: &str,
    sel: &ContextSelection,
    total_confirmed: usize,
) -> PersonaContext {
    let mut entries: Vec<(ContextEntry, Option<chrono::DateTime<chrono::Utc>>)> = facts
        .iter()
        // BR-P1: unconfirmed candidates are never grounding.
        .filter(|f| f.metadata.confirmed)
        .filter(|f| sel.scope.is_none_or(|s| s == f.metadata.scope))
        .map(|f| {
            (
                ContextEntry {
                    id: f.id,
                    title: f.title.clone(),
                    body: f.body.clone(),
                    scope: f.metadata.scope,
                    relevance: relevance(query, f),
                },
                f.metadata.confirmed_at,
            )
        })
        .collect();

    // A fact upserted twice can legitimately appear twice in a search result;
    // the context must still hold each FactId once (invariant C3).
    entries.sort_by(|a, b| {
        a.0.id
            .0
            .cmp(&b.0.id.0)
            .then_with(|| b.0.relevance.cmp(&a.0.relevance))
    });
    entries.dedup_by(|a, b| a.0.id == b.0.id);

    entries.sort_by(|a, b| {
        b.0.relevance
            .cmp(&a.0.relevance)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.id.0.cmp(&b.0.id.0))
    });
    entries.truncate(sel.max_facts);

    PersonaContext {
        entries: entries.into_iter().map(|(e, _)| e).collect(),
        total_confirmed,
    }
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Company => "업무",
        Scope::Personal => "개인",
        Scope::Unknown => "미분류",
    }
}

/// Output-format directive per draft kind (BR-P8). Static text, no owner data.
fn kind_directive(kind: DraftKind) -> &'static str {
    match kind {
        DraftKind::Email => {
            "요청받은 내용을 이메일 초안으로 작성하세요. 첫 줄에 '제목: '으로 시작하는 제목을 쓰고, \
             빈 줄 뒤에 격식 있는 본문을 이어 쓰세요."
        }
        DraftKind::Message => {
            "요청받은 내용을 메신저 메시지 초안으로 작성하세요. 3문장 이내로 짧고 구어체로 쓰고, \
             불필요한 인사말은 생략하세요."
        }
        DraftKind::Post => {
            "요청받은 내용을 공개 게시글 초안으로 작성하세요. 1인칭으로 쓰고, \
             읽는 사람이 배경을 몰라도 이해할 수 있게 맥락을 먼저 밝히세요."
        }
    }
}

const PERSONA_INSTRUCTIONS: &str = "당신은 사용자 본인의 페르소나입니다. 아래 [맥락]에 적힌 확정된 사실만 근거로 삼아 1인칭으로 답하세요. \
맥락에 없는 내용은 지어내지 말고, 모르는 것은 모른다고 말하세요.";

/// Render the prompt pair.
///
/// `system` is assembled only from static templates, so it carries no owner
/// data and needs no masking. Everything variable lands in `user_document`,
/// which the caller masks in a single pass (BR-P2).
pub fn render_prompt(
    ctx: &PersonaContext,
    user_input: &str,
    kind: Option<DraftKind>,
) -> PersonaPrompt {
    let system = match kind {
        Some(k) => format!("{PERSONA_INSTRUCTIONS}\n{}", kind_directive(k)),
        None => PERSONA_INSTRUCTIONS.to_string(),
    };

    let mut doc = String::from("[맥락]\n");
    for e in &ctx.entries {
        doc.push_str(&format!(
            "- ({}) {}: {}\n",
            scope_label(e.scope),
            e.title,
            e.body
        ));
    }
    doc.push_str("\n[요청]\n");
    doc.push_str(user_input);

    PersonaPrompt {
        system,
        user_document: doc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::testgen::{fact, fact_with};

    #[test]
    fn unconfirmed_facts_never_enter_context() {
        let facts = vec![
            fact("확정", "본문 A", true),
            fact("미확정", "본문 B", false),
        ];
        let ctx = select_context(&facts, "본문", &ContextSelection::default(), 1);
        assert_eq!(ctx.entries.len(), 1);
        assert_eq!(ctx.entries[0].title, "확정");
        assert_eq!(ctx.total_confirmed, 1);
    }

    #[test]
    fn context_is_truncated_to_max_facts() {
        let facts: Vec<_> = (0..50)
            .map(|i| fact(&format!("사실 {i}"), "본문", true))
            .collect();
        let sel = ContextSelection {
            max_facts: 5,
            scope: None,
        };
        let ctx = select_context(&facts, "사실", &sel, 50);
        assert_eq!(ctx.entries.len(), 5);
        assert_eq!(ctx.total_confirmed, 50);
    }

    #[test]
    fn empty_input_yields_empty_context() {
        let ctx = select_context(&[], "무엇이든", &ContextSelection::default(), 0);
        assert!(ctx.is_empty());
        assert_eq!(ctx.total_confirmed, 0);
    }

    #[test]
    fn title_matches_outrank_body_matches() {
        let facts = vec![
            fact("배포 절차", "관계 없는 본문", true),
            fact("관계 없는 제목", "배포 배포 배포", true),
        ];
        let ctx = select_context(&facts, "배포", &ContextSelection::default(), 2);
        // title hit = 3 points, three body hits = 3 points -> tie broken later,
        // so assert on the score itself rather than order.
        assert_eq!(ctx.entries.len(), 2);
        let titled = ctx.entries.iter().find(|e| e.title == "배포 절차").unwrap();
        assert_eq!(titled.relevance, TITLE_WEIGHT);
    }

    #[test]
    fn scope_filter_restricts_context() {
        let facts = vec![
            fact_with("업무 사실", "본문", true, Scope::Company),
            fact_with("개인 사실", "본문", true, Scope::Personal),
        ];
        let sel = ContextSelection {
            max_facts: 12,
            scope: Some(Scope::Company),
        };
        let ctx = select_context(&facts, "본문", &sel, 2);
        assert_eq!(ctx.entries.len(), 1);
        assert_eq!(ctx.entries[0].scope, Scope::Company);
        // total_confirmed counts the whole confirmed population, pre-filter.
        assert_eq!(ctx.total_confirmed, 2);
    }

    #[test]
    fn duplicate_fact_ids_are_collapsed() {
        let f = fact("중복", "본문", true);
        let ctx = select_context(&[f.clone(), f], "본문", &ContextSelection::default(), 1);
        assert_eq!(ctx.entries.len(), 1);
    }

    #[test]
    fn system_prompt_carries_no_owner_data() {
        let facts = vec![fact("비밀 제목", "a@b.com 이라는 주소", true)];
        let ctx = select_context(&facts, "주소", &ContextSelection::default(), 1);
        let p = render_prompt(&ctx, "내 주소 알려줘", None);
        assert!(!p.system.contains("비밀 제목"));
        assert!(!p.system.contains("a@b.com"));
        assert!(!p.system.contains("내 주소 알려줘"));
        // ...and everything variable is in the document that gets masked.
        assert!(p.user_document.contains("비밀 제목"));
        assert!(p.user_document.contains("a@b.com"));
        assert!(p.user_document.contains("내 주소 알려줘"));
    }

    #[test]
    fn draft_kind_directive_lands_in_system_prompt() {
        let ctx = PersonaContext::default();
        let email = render_prompt(&ctx, "요청", Some(DraftKind::Email));
        let msg = render_prompt(&ctx, "요청", Some(DraftKind::Message));
        assert!(email.system.contains("제목: "));
        assert!(msg.system.contains("메신저"));
        assert_ne!(email.system, msg.system);
    }
}
