# U2 Functional Design Plan (Ingestion & Processing)

> Owner: Dev B · Branch: `construction/u2-ingestion-processing`
> 대상 스토리: US-1.1~1.5(수집), US-2.1~2.3(가공/마스킹/투명성)
> 의존(mock 준비됨): U1 `Masker`/`LlmClient`/`CredentialStore`/`EncryptedStore`, U3 `KnowledgeApi.upsert`/`InterviewApi.enqueue`
> 활성 확장: Property-Based Testing (Partial — PBT-02/03/07/08/09 blocking)

## 설계 산출물 체크리스트 (Step 6에서 생성)
- [ ] `functional-design/domain-entities.md` — U2 내부 도메인 엔티티(수집 커서, 전송로그, 가공 결정)와 공유 타입과의 관계
- [ ] `functional-design/business-logic-model.md` — 수집 파이프라인(증분·멱등) + 가공 파이프라인(마스킹→요약·분류→라우팅) 흐름
- [ ] `functional-design/business-rules.md` — 멱등 규칙, "내 것" 필터, 잡음/불확실 판정, 마스킹 정책, 투명성 로깅, 오프라인 저하
- [ ] PBT-01: 속성 식별(멱등성·마스킹 불변식·마스킹 왕복) 문서화 (본 단계에서 식별, 테스트는 Code Generation)

---

## 명확화 질문 (해당 [Answer]: 태그를 채워주세요)

형식: A/B/C/D 중 선택하거나 자유 서술. 애매하면 추천안(★)을 기본으로 진행합니다.

### Q1. 증분·멱등의 "이미 수집됨" 판정 키 (US-1.1 AC2, NFR-5)
어떤 기준으로 중복을 판정할까요?
- A. `(SourceKind, external_id)` 조합만으로 판정 (external_id가 소스별 안정 ID) ★
- B. `(SourceKind, external_id, content_hash)` — 내용이 바뀌면 재수집(변경분 반영)
- C. 소스마다 다름 (세션=파일경로+offset, Notion/Gmail=API id+last_edited)

[Answer]:

### Q2. 세션 수집 대상 경로 (US-1.1, FR-1.1)
Claude Code / Codex 세션 트랜스크립트를 어디서 읽나요?
- A. 알려진 기본 경로 자동 탐지 + 사용자 설정으로 override (SourceConfig) ★
- B. 사용자가 반드시 경로를 지정해야 함(자동 탐지 없음)
- C. 기타(서술):

[Answer]:

### Q3. "내 것" 필터 판정 (US-1.3 Notion / US-1.4 Gmail)
- A. Gmail=내 주소 기준 보낸/받은 메일, Notion=내가 소유/편집자인 페이지 (커넥터별 규칙 내장) ★
- B. 단순화: 인증된 계정으로 접근 가능한 전부(별도 소유자 필터 없음)
- C. 기타(서술):

[Answer]:

### Q4. 잡음/불확실 판정 주체 (US-2.1 AC2/AC3)
"저장 vs 필터 vs Queue 이관"을 무엇이 결정하나요?
- A. LLM 분류 결과(`LlmClient.classify`)로 결정하되, U2는 임계·규칙으로 라우팅(불확실=Queue, 잡음=drop, 확실=Fact) ★
- B. U2 자체 휴리스틱(키워드/빈도)만으로 결정, LLM은 요약만
- C. 기타(서술):

[Answer]:

### Q5. 마스킹 매핑(UnmaskMap)의 수명/저장 (US-2.2 AC2)
`UnmaskMap`은 원문 복원용 매핑입니다(기기 밖 유출 금지).
- A. 가공 처리 중에만 메모리 보관, 처리 끝나면 폐기(복원은 그 세션 내에서만) ★
- B. 사실(Fact)과 함께 `EncryptedStore`에 암호화 저장하여 나중에도 복원 가능
- C. 기타(서술):

[Answer]:

### Q6. 전송 투명성 로그(TransferLog) 저장 위치 (US-2.3)
- A. `EncryptedStore`에 append-only로 저장(마스킹 후 요약/대상/시각) ★
- B. 별도 파일 로그
- C. 기타(서술):

[Answer]:

### Q7. 파일 수집 지원 형식 (US-1.5)
MVP에서 파싱할 형식 범위는?
- A. 텍스트(.txt/.md), 오피스(.docx/.pdf), 이미지(.png/.jpg→비전). 그 외는 스킵+사유기록 ★
- B. 텍스트+이미지만 (오피스는 향후)
- C. 기타(서술):

[Answer]:

### Q8. 오프라인 저하 동작 (NFR-3)
LLM 미연결 시 가공(요약/분류/비전)은?
- A. 수집·원본 대기는 계속, 가공은 "대기(pending)"로 큐잉하고 온라인 시 재개 ★
- B. 가공 시도 후 실패 기록, 다음 배치에서 재시도
- C. 기타(서술):

[Answer]:
