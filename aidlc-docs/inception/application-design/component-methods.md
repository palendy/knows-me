# Component Methods (knows-me)

> 고수준 메서드 시그니처(목적·입출력). **상세 비즈니스 규칙은 Functional Design(Unit별)에서 확정.**
> 표기는 Rust 유사 의사코드(프런트는 Tauri command 이름). 오류는 `Result<T, AppError>`로 반환한다고 가정.

## Command/App Layer (Tauri commands — 프런트 ↔ 코어)
```rust
// 온보딩/보안
setup_password(password: String) -> Result<()>            // 최초 키 초기화
unlock(password: String) -> Result<()>                     // 키 재유도, AppState 잠금 해제
lock() -> Result<()>

// 조회
get_dashboard() -> DashboardDto                            // 수집현황·Queue수·최근 사실
get_minihome() -> MiniHomeDto
get_graph(filter: GraphFilter) -> GraphDto
search_facts(query: String, filters: FactFilter) -> Vec<FactSummary>

// 인터뷰 Queue
list_queue(sort: QueueSort) -> Vec<QueueItemDto>
answer_item(item_id: Id, answer: AnswerInput) -> AnswerResult  // 확정 사실 반영/후속질문

// 수집/설정
configure_source(source: SourceKind, config: SourceConfig) -> Result<()>
trigger_ingest(source: Option<SourceKind>) -> IngestReport   // "지금 수집"
set_transfer_policy(policy: TransferPolicy) -> Result<()>
set_server_enabled(on: bool) -> Result<()>

// 페르소나
persona_chat(prompt: String) -> PersonaReply
```
*모든 command는 CommandRouter에서 잠금 상태·입력 검증 후 서비스로 위임.*

## C. Ingestion
```rust
trait Connector {
    fn id(&self) -> SourceKind;
    fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)>; // 증분
    fn supports_manual(&self) -> bool;
}
FileConnector::ingest_file(path: Path) -> Result<Vec<RawItem>>       // 수동 업로드/비전
CursorStore::get(source: SourceKind) -> Option<Cursor>
CursorStore::set(source: SourceKind, cursor: Cursor) -> Result<()>
```

## D. Processing
```rust
Masker::mask(text: &str) -> (MaskedText, UnmaskMap)      // 식별정보/비밀 치환
Masker::unmask(t: &MaskedText, m: &UnmaskMap) -> String  // 로컬 전용 복원
LlmClient::summarize(masked: &MaskedText) -> Result<Summary>
LlmClient::classify(masked: &MaskedText) -> Result<Vec<Label>>
LlmClient::vision_extract(image: &Bytes) -> Result<MaskedText>
LlmClient::chat(context: &PersonaContext, prompt: &MaskedText) -> Result<String>
SummarizerClassifier::process(raw: Vec<RawItem>) -> Result<Vec<FactCandidate>>
TransferLog::record(call: TransferRecord) -> Result<()>
```

## E. Knowledge
```rust
FactStore::upsert(fact: Fact) -> Result<Id>              // 문서(md/json) 쓰기 + 링크
FactStore::get(id: Id) -> Result<Fact>
FactStore::links(id: Id) -> Vec<Id>
HistoryTracker::append(id: Id, change: FactChange) -> Result<()>  // 이력 보존
HistoryTracker::history(id: Id) -> Vec<FactChange>
SearchIndex::index(fact: &Fact) -> Result<()>
SearchIndex::search(query: &str, filters: FactFilter) -> Vec<FactSummary>
```

## F. Interview
```rust
QueueManager::enqueue(item: QueueItem) -> Result<Id>     // 확인형/심화형
QueueManager::list(sort: QueueSort) -> Vec<QueueItem>
QueueManager::expire() -> Result<usize>                  // 만료/억제
AnswerIntake::answer(item_id: Id, answer: AnswerInput) -> Result<AnswerResult>
// AnswerResult: 확정 사실(Option<Fact>) + 후속 질문(Vec<QueueItem>)
```

## G. Persona
```rust
PersonaService::build_context() -> PersonaContext        // 확정 맥락 조합
PersonaService::chat(prompt: &str) -> Result<PersonaReply>
PersonaService::draft(req: DraftRequest) -> Result<Draft> // "나 대신 네트워킹"
LocalApiServer::start(port: u16) -> Result<()>           // routes: POST /draft, POST /chat
LocalApiServer::stop() -> Result<()>
```

## H. Security
```rust
KeyManager::setup(password: &str) -> Result<()>          // KDF salt 생성·검증자 저장
KeyManager::unlock(password: &str) -> Result<KeyHandle>  // 키 재유도(메모리)
KeyManager::lock()
Vault::encrypt(plain: &[u8], key: &KeyHandle) -> Vec<u8> // AES-256-GCM
Vault::decrypt(cipher: &[u8], key: &KeyHandle) -> Result<Vec<u8>>
Vault::store_credential(source: SourceKind, cred: Credential, key: &KeyHandle) -> Result<()>
Vault::load_credential(source: SourceKind, key: &KeyHandle) -> Result<Credential>
```

> **PBT 대상(NFR-8)**: `Masker::mask/unmask`(왕복·불변식), `Vault::encrypt/decrypt`(왕복), `FactStore` 직렬화/역직렬화(왕복). 상세는 Functional Design PBT-01에서 식별.
