//! `PersonaApi` implementation — US-6.1 chat, US-6.2 drafting.
//!
//! The pipeline is fixed and its order is load-bearing:
//!
//! ```text
//! search -> select_context -> render_prompt -> mask -> llm -> unmask
//!                                              ^^^^
//!                          nothing variable reaches the cloud unmasked (BR-P2)
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::error::{AppError, Result};
use crate::core::traits::{KnowledgeApi, LlmClient, Masker, PersonaApi};
use crate::core::types::{Draft, DraftKind, DraftRequest, Fact, FactFilter, PersonaReply};

use super::context::{render_prompt, select_context, title_relevance};
use super::{
    ContextSelection, PersonaContext, DEFAULT_FETCH_CAP, MAX_PROMPT_CHARS, NO_CONTEXT_REPLY,
};

/// Persona avatar backed by confirmed knowledge plus U1's masked LLM gateway.
pub struct PersonaService {
    knowledge: Arc<dyn KnowledgeApi>,
    masker: Arc<dyn Masker>,
    llm: Arc<dyn LlmClient>,
    selection: ContextSelection,
    fetch_cap: usize,
}

impl PersonaService {
    pub fn new(
        knowledge: Arc<dyn KnowledgeApi>,
        masker: Arc<dyn Masker>,
        llm: Arc<dyn LlmClient>,
    ) -> Self {
        Self {
            knowledge,
            masker,
            llm,
            selection: ContextSelection::default(),
            fetch_cap: DEFAULT_FETCH_CAP,
        }
    }

    /// Override the context policy (size cap, scope restriction).
    pub fn with_selection(mut self, selection: ContextSelection) -> Self {
        self.selection = selection;
        self
    }

    /// Assemble the grounding context for a prompt.
    ///
    /// Read amplification is bounded: at most `fetch_cap` facts are fetched in
    /// full before relevance is refined over title *and* body (U4-NFR-P5).
    pub async fn build_context(&self, prompt: &str) -> Result<PersonaContext> {
        let filter = FactFilter {
            scope: self.selection.scope,
        };

        // Targeted hits first. The confirmed filter runs *before* any decision
        // is made on the count — otherwise a pile of unconfirmed matches would
        // look like good recall, suppress the widening below, and then vanish,
        // reporting "no context" while confirmed facts sat there unread.
        let mut candidates = self
            .knowledge
            .search(prompt.to_string(), filter.clone())
            .await?;
        candidates.retain(|s| s.confirmed);

        // The confirmed population, from summaries alone — no per-fact fetch.
        // It is both the honest denominator for `total_confirmed` (E1) and the
        // widening pool when the store's query semantics are stricter than a
        // natural question needs them to be.
        let mut population = self.knowledge.search(String::new(), filter).await?;
        population.retain(|s| s.confirmed);
        let total_confirmed = population.len();

        if candidates.len() < self.selection.max_facts {
            candidates.extend(population);
        }

        // Rank by what a summary can tell us (title relevance) before spending
        // the fetch budget. Truncating on id order instead would throw away the
        // most relevant facts whenever the confirmed set exceeds `fetch_cap`.
        candidates.sort_by_key(|s| s.id.0);
        candidates.dedup_by(|a, b| a.id == b.id);
        candidates.sort_by(|a, b| {
            title_relevance(prompt, &b.title)
                .cmp(&title_relevance(prompt, &a.title))
                .then_with(|| a.id.0.cmp(&b.id.0))
        });
        candidates.truncate(self.fetch_cap);

        let mut facts: Vec<Fact> = Vec::with_capacity(candidates.len());
        for s in candidates {
            if let Ok(f) = self.knowledge.get(s.id).await {
                facts.push(f);
            }
        }

        Ok(select_context(
            &facts,
            prompt,
            &self.selection,
            total_confirmed,
        ))
    }

    /// The one place where anything leaves the device.
    async fn ground_and_ask(&self, user_input: &str, kind: Option<DraftKind>) -> Result<String> {
        if user_input.trim().is_empty() {
            return Err(AppError::InvalidInput("빈 요청입니다".into()));
        }
        if user_input.chars().count() > MAX_PROMPT_CHARS {
            return Err(AppError::InvalidInput(format!(
                "요청이 너무 깁니다 (최대 {MAX_PROMPT_CHARS}자)"
            )));
        }

        let ctx = self.build_context(user_input).await?;
        // BR-P4: no grounding means no cloud call at all.
        if ctx.is_empty() {
            return Ok(NO_CONTEXT_REPLY.to_string());
        }

        let prompt = render_prompt(&ctx, user_input, kind);
        // Single mask pass: `system` is a static template with no owner data,
        // so one call yields one UnmaskMap and no placeholder collisions.
        let (masked, unmask_map) = self.masker.mask(&prompt.user_document);

        let raw = self.llm.chat(&prompt.system, &masked).await?;

        // Restoration is local-only; `unmask_map` is dropped at end of scope
        // and never persisted or transmitted (BR-P3).
        Ok(self
            .masker
            .unmask(&crate::core::types::MaskedText { text: raw }, &unmask_map))
    }
}

#[async_trait]
impl PersonaApi for PersonaService {
    async fn chat(&self, prompt: String) -> Result<PersonaReply> {
        let text = self.ground_and_ask(&prompt, None).await?;
        Ok(PersonaReply { text })
    }

    async fn draft(&self, req: DraftRequest) -> Result<Draft> {
        let text = self.ground_and_ask(&req.prompt, Some(req.kind)).await?;
        Ok(Draft { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::MaskedText;
    use crate::mocks::{CannedLlm, InMemoryKnowledge, NoopMasker};
    use crate::persona::testgen::fact;
    use std::sync::Mutex;

    /// Records what the LLM gateway was handed, so tests can assert on it.
    #[derive(Default)]
    struct SpyLlm {
        calls: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl LlmClient for SpyLlm {
        async fn summarize(&self, _input: &MaskedText) -> Result<String> {
            Ok(String::new())
        }
        async fn classify(&self, _input: &MaskedText) -> Result<Vec<String>> {
            Ok(vec![])
        }
        async fn vision_extract(&self, _image_png: &[u8]) -> Result<MaskedText> {
            Ok(MaskedText {
                text: String::new(),
            })
        }
        async fn chat(&self, system: &str, input: &MaskedText) -> Result<String> {
            self.calls
                .lock()
                .unwrap()
                .push((system.to_string(), input.text.clone()));
            Ok("응답".to_string())
        }
    }

    /// Always fails, standing in for "offline" / provider outage.
    struct FailingLlm;

    #[async_trait]
    impl LlmClient for FailingLlm {
        async fn summarize(&self, _input: &MaskedText) -> Result<String> {
            Err(AppError::External("offline".into()))
        }
        async fn classify(&self, _input: &MaskedText) -> Result<Vec<String>> {
            Err(AppError::External("offline".into()))
        }
        async fn vision_extract(&self, _image_png: &[u8]) -> Result<MaskedText> {
            Err(AppError::External("offline".into()))
        }
        async fn chat(&self, _system: &str, _input: &MaskedText) -> Result<String> {
            Err(AppError::External("offline".into()))
        }
    }

    /// Replaces every "@" so tests can prove masking ran before the call.
    struct AtSignMasker;

    impl Masker for AtSignMasker {
        fn mask(&self, text: &str) -> (MaskedText, crate::core::types::UnmaskMap) {
            let mut map = crate::core::types::UnmaskMap::new();
            let mut out = String::new();
            for tok in text.split_whitespace() {
                if tok.contains('@') {
                    let ph = format!("[EMAIL_{}]", map.entries().len());
                    map.insert(ph.clone(), tok.to_string());
                    out.push_str(&ph);
                } else {
                    out.push_str(tok);
                }
                out.push(' ');
            }
            (MaskedText { text: out }, map)
        }
        fn unmask(&self, masked: &MaskedText, map: &crate::core::types::UnmaskMap) -> String {
            let mut s = masked.text.clone();
            for (ph, orig) in map.entries() {
                s = s.replace(ph, orig);
            }
            s
        }
    }

    async fn seeded_knowledge() -> Arc<InMemoryKnowledge> {
        let kn = Arc::new(InMemoryKnowledge::default());
        kn.upsert(fact("배포 절차", "make deploy 로 배포한다", true))
            .await
            .unwrap();
        kn
    }

    #[tokio::test]
    async fn empty_context_short_circuits_without_calling_the_llm() {
        let llm = Arc::new(SpyLlm::default());
        let svc = PersonaService::new(
            Arc::new(InMemoryKnowledge::default()),
            Arc::new(NoopMasker),
            llm.clone(),
        );

        let reply = svc.chat("배포 어떻게 해?".into()).await.unwrap();

        assert_eq!(reply.text, NO_CONTEXT_REPLY);
        assert!(
            llm.calls.lock().unwrap().is_empty(),
            "LLM must not be called"
        );
    }

    #[tokio::test]
    async fn unconfirmed_facts_do_not_ground_an_answer() {
        let kn = Arc::new(InMemoryKnowledge::default());
        kn.upsert(fact("배포 절차", "확정되지 않은 내용", false))
            .await
            .unwrap();
        let llm = Arc::new(SpyLlm::default());
        let svc = PersonaService::new(kn, Arc::new(NoopMasker), llm.clone());

        let reply = svc.chat("배포".into()).await.unwrap();

        assert_eq!(reply.text, NO_CONTEXT_REPLY);
        assert!(llm.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn context_reaches_the_llm_as_masked_input() {
        let llm = Arc::new(SpyLlm::default());
        let svc = PersonaService::new(seeded_knowledge().await, Arc::new(NoopMasker), llm.clone());

        svc.chat("배포".into()).await.unwrap();

        let calls = llm.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (system, input) = &calls[0];
        assert!(input.contains("make deploy"), "context must be grounded");
        assert!(
            !system.contains("make deploy"),
            "system carries no owner data"
        );
    }

    #[tokio::test]
    async fn identifiers_are_masked_before_the_call_and_restored_after() {
        let kn = Arc::new(InMemoryKnowledge::default());
        kn.upsert(fact("연락처", "hong@example.com 으로 연락", true))
            .await
            .unwrap();
        let llm = Arc::new(SpyLlm::default());
        let svc = PersonaService::new(kn, Arc::new(AtSignMasker), llm.clone());

        svc.chat("연락처 알려줘".into()).await.unwrap();

        let calls = llm.calls.lock().unwrap();
        let (_, input) = &calls[0];
        assert!(
            !input.contains("hong@example.com"),
            "raw identifier must not reach the gateway"
        );
        assert!(input.contains("[EMAIL_0]"));
    }

    #[tokio::test]
    async fn llm_failure_propagates_as_external() {
        let svc = PersonaService::new(
            seeded_knowledge().await,
            Arc::new(NoopMasker),
            Arc::new(FailingLlm),
        );

        let err = svc.chat("배포".into()).await.unwrap_err();
        assert!(matches!(err, AppError::External(_)));
    }

    #[tokio::test]
    async fn blank_and_oversized_prompts_are_rejected() {
        let svc = PersonaService::new(
            seeded_knowledge().await,
            Arc::new(NoopMasker),
            Arc::new(CannedLlm),
        );

        assert!(matches!(
            svc.chat("   ".into()).await.unwrap_err(),
            AppError::InvalidInput(_)
        ));
        assert!(matches!(
            svc.chat("가".repeat(MAX_PROMPT_CHARS + 1))
                .await
                .unwrap_err(),
            AppError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn draft_kind_changes_the_system_directive() {
        let llm = Arc::new(SpyLlm::default());
        let kn = seeded_knowledge().await;
        let svc = PersonaService::new(kn, Arc::new(NoopMasker), llm.clone());

        svc.draft(DraftRequest {
            kind: DraftKind::Email,
            prompt: "배포 일정 공유 메일".into(),
        })
        .await
        .unwrap();
        svc.draft(DraftRequest {
            kind: DraftKind::Message,
            prompt: "배포 일정 공유 메시지".into(),
        })
        .await
        .unwrap();

        let calls = llm.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].0, calls[1].0, "directives must differ by kind");
        assert!(calls[0].0.contains("제목: "));
    }

    #[tokio::test]
    async fn context_respects_the_configured_size_cap() {
        let kn = Arc::new(InMemoryKnowledge::default());
        for i in 0..30 {
            kn.upsert(fact(&format!("배포 {i}"), "본문", true))
                .await
                .unwrap();
        }
        let svc = PersonaService::new(kn, Arc::new(NoopMasker), Arc::new(CannedLlm))
            .with_selection(ContextSelection {
                max_facts: 3,
                scope: None,
            });

        let ctx = svc.build_context("배포").await.unwrap();
        assert_eq!(ctx.entries.len(), 3);
        assert_eq!(ctx.total_confirmed, 30);
    }
}
