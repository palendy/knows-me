# Components (knows-me)

> 고수준 컴포넌트 정의·책임·인터페이스. 상세 비즈니스 로직은 Functional Design(Unit별)에서.
> **스택**: React+TS 프런트(Tauri 웹뷰) · Rust 코어 단일(Tauri commands) · Rust 내장 HTTP 서버 · 계층형+서비스 오케스트레이션.

## 아키텍처 개요 (계층)
```
[ Frontend (React/TS) ]  UI 뷰 — 명령/조회만 (Tauri invoke)
        |  Tauri commands (IPC)
[ Command/App Layer (Rust) ]  요청 검증·서비스 라우팅
        |
[ Domain Services (Rust) ]  Ingestion / Processing / Knowledge / Interview / Persona / Security
        |
[ Components (Rust) ]  Connector*, Masker, LlmClient, FactStore, QueueManager, Vault ...
        |
[ Local Store (암호화 파일 위키) ] + [ Local HTTP Server ] + [ External APIs ]
```

---

## A. Frontend 컴포넌트 (React/TS — thin, 코어 호출만)
| 컴포넌트 | 목적/책임 | 주요 인터페이스(호출) |
|---|---|---|
| **OnboardingView** | 최초 설치 비밀번호 설정 · 잠금 해제 | `setup_password`, `unlock` |
| **DashboardView** | 수집 현황·대기 Queue 수·최근 확정 사실 표시 | `get_dashboard` |
| **InterviewQueueView** | Queue 목록 + **전환 없는** 답변(선택지+직접입력) | `list_queue`, `answer_item` |
| **MiniHomeView** | 확정 맥락을 미니홈피 스타일 시각화 | `get_minihome` |
| **GraphView** | 사실 문서 링크를 노드/엣지 그래프로 탐색 | `get_graph` |
| **PersonaChatView** | 페르소나 아바타와 대화(소유자) | `persona_chat` |
| **SettingsView** | 소스 연동·감시 폴더·전송 정책·서버 토글 | `configure_source`, `set_transfer_policy` |

## B. Command/App Layer (Rust)
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **CommandRouter** | Tauri command 진입점. 입력 검증 후 도메인 서비스로 위임. 잠금 상태 가드. | 위 프런트 호출에 1:1 대응하는 `#[tauri::command]` 함수 |
| **AppState** | 앱 전역 상태(잠금 여부, 열린 키 핸들, 설정) 보관 | (내부) |
| **Scheduler** | 주기적 배치 트리거(세션·외부 동기화) | `start`, `tick` |

## C. Ingestion 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **Connector (trait)** | 모든 소스 커넥터의 공통 계약(플러그형) | `id()`, `sync(cursor) -> RawItems`, `supports_manual()` |
| **SessionConnector** | Claude Code/Codex 세션 트랜스크립트 증분 수집 | Connector 구현 |
| **NotionConnector** | Notion "내 것" 페이지 증분 수집 | Connector 구현 |
| **GmailConnector** | 내가 주고받은 메일 증분 수집 | Connector 구현 |
| **FileConnector** | 감시 폴더 + 수동 업로드, 이미지 비전 추출 | Connector 구현 + `ingest_file(path)` |
| **CursorStore** | 소스별 수집 커서(증분·멱등) 저장 | `get(source)`, `set(source, cursor)` |

## D. Processing 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **Masker** | 클라우드 전송 전 식별정보·비밀 제거/치환, 로컬 복원 매핑 | `mask(text) -> (masked, map)`, `unmask(masked, map)` |
| **LlmClient** | 클라우드 LLM 호출(요약·분류·비전·대화). 마스킹된 입력만 전송 | `summarize`, `classify`, `vision_extract`, `chat` |
| **SummarizerClassifier** | 반복/필요 항목 선별 → 요약·분류하여 사실 후보 생성 | `process(raw) -> Vec<FactCandidate>` |
| **TransferLog** | 외부로 나간(마스킹 후) 요약/대상 기록(투명성) | `record(call)`, `list()` |

## E. Knowledge 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **FactStore** | 사실=문서(마크다운/JSON) 파일 위키, 문서 간 링크, 메타데이터 | `upsert(fact)`, `get(id)`, `links(id)` |
| **HistoryTracker** | 사실 갱신 시 덮어쓰지 않고 이력 보존 | `append(id, change)`, `history(id)` |
| **SearchIndex** | 키워드/메타데이터 검색 인덱스 | `index(fact)`, `search(query, filters)` |

## F. Interview 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **QueueManager** | 확인형/심화형 아이템 생성, 우선순위·만료, 후속 질문 연쇄 | `enqueue(item)`, `list(sort)`, `expire()` |
| **AnswerIntake** | 답변(선택지/직접입력) 수용 → 확정 사실로 전환, 후속 질문 트리거 | `answer(item_id, answer) -> Option<Fact>` |

## G. Persona 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **PersonaService** | 확정 맥락으로 페르소나 컨텍스트 구성, 응답·초안 생성 | `chat(prompt)`, `draft(request)` |
| **LocalApiServer** | 앱 내장 HTTP 서버(localhost). "나 대신 네트워킹" REST 제공. MVP 외부 노출 없음 | `start(port)`, `stop()`, routes: `POST /draft`, `POST /chat` |

## H. Security 컴포넌트
| 컴포넌트 | 목적/책임 | 인터페이스 |
|---|---|---|
| **KeyManager** | 비밀번호 → KDF(Argon2/PBKDF2) → 대칭키. 평문 키 미저장, 잠금/해제 | `setup(password)`, `unlock(password) -> KeyHandle`, `lock()` |
| **Vault** | 저장 데이터·자격증명 암호화/복호화(AES-256-GCM) | `encrypt(bytes)`, `decrypt(bytes)`, `store_credential`, `load_credential` |

## 컴포넌트 ↔ Epic/스토리 매핑
- A(Frontend)·B ↔ E5, E6, E7 (UI/온보딩)
- C(Ingestion) ↔ E1 (US-1.1~1.5)
- D(Processing) ↔ E2 (US-2.1~2.3)
- E(Knowledge) ↔ E3 (US-3.1~3.3)
- F(Interview) ↔ E4 (US-4.1~4.3)
- G(Persona) ↔ E6 (US-6.1~6.2)
- H(Security) ↔ E7 (US-7.1~7.3)
