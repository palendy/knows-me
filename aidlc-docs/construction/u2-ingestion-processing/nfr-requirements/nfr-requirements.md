# U2 NFR Requirements — Ingestion & Processing

> 개인용 로컬 우선 데스크탑 앱(Tauri) PoC. U2는 수집·가공 백엔드 로직 전담.
> 상위 NFR(요구사항서 §5) 중 U2에 해당하는 것을 구체화한다.

## 1. 프라이버시·보안 (NFR-2, R1) — 최상위 우선순위
| ID | 요구사항 | 측정/검증 |
|---|---|---|
| U2-NFR-SEC1 | 외부 LLM 전송 전 모든 텍스트는 `Masker.mask` 통과(원문 직접 전송 0건). | 코드 경로 검증 + PBT-03 불변식 |
| U2-NFR-SEC2 | 마스킹 출력·TransferLog에 원본 식별정보 잔존 0. | PBT-03(생성된 식별정보가 출력에 부재) |
| U2-NFR-SEC3 | `UnmaskMap`은 저장·전송 금지, 처리 스코프 종료 시 폐기. | 코드 리뷰 + 저장소 ns에 부재 |
| U2-NFR-SEC4 | 자격증명은 `CredentialStore`(U1 Vault)에만 암호화 보관, 평문/하드코딩 0. | 코드 스캔(심사기준⑥) |
| U2-NFR-SEC5 | 마스킹 강도: 정규식(이메일·전화·API키/토큰·URL 자격증명) + 사용자 사전(이름 등). 미검출은 패턴/위치만 로깅. | 규칙 커버리지 테스트 |

## 2. 신뢰성·오류 처리 (NFR-3, US-1.1 AC3)
| ID | 요구사항 | 측정/검증 |
|---|---|---|
| U2-NFR-REL1 | 한 소스 실패가 다른 소스/앱을 중단시키지 않음(격리). | 오류 주입 테스트 |
| U2-NFR-REL2 | LLM 호출: 타임아웃 + 지수 백오프 제한 재시도 후 실패 시 `pending` 이월. | 재시도 로직 단위 테스트 |
| U2-NFR-REL3 | 부분 실패 시 커서 미전진·seen 미기록 → 다음 주기 재시도 가능(데이터 유실 0). | 멱등 재실행 테스트 |
| U2-NFR-REL4 | 인증 만료는 오류가 아니라 재인증 유도 신호로 처리, 앱 계속. | 커넥터 테스트 |

## 3. 오프라인/저하 (NFR-3)
| ID | 요구사항 | 측정/검증 |
|---|---|---|
| U2-NFR-OFF1 | LLM 미연결 시 수집·원본 대기 계속 동작. | 오프라인 시뮬레이션 |
| U2-NFR-OFF2 | 가공은 `PendingProcessingItem`으로 큐잉, 온라인 복귀 시 재개. | pending drain 테스트 |
| U2-NFR-OFF3 | 오프라인은 errors가 아닌 pending(지연)으로 계측. | 카운터 검증 |

## 4. 멱등·증분 (NFR-5)
| ID | 요구사항 | 측정/검증 |
|---|---|---|
| U2-NFR-IDEM1 | `trigger` 재실행 시 신규 없으면 collected=0(멱등). | PBT-03 멱등성 |
| U2-NFR-IDEM2 | 판정 키 `(SourceKind, external_id)` 안정성. | 커넥터별 external_id 계약 테스트 |

## 5. 성능 (개인 PoC — Q2=A)
| ID | 요구사항 |
|---|---|
| U2-NFR-PERF1 | 명시적 처리량/지연 SLA 없음. 목표: 체감 무한정 대기 없음. 배치는 백그라운드, 수동 트리거는 진행 표시. |
| U2-NFR-PERF2 | LLM 왕복 지연은 외부 요인으로 간주(재시도/타임아웃으로만 관리). |
| U2-NFR-PERF3 | 수집은 증분이므로 데이터 누적에도 1회 처리량은 신규분에 비례(전량 재스캔 지양). |

## 6. 확장성·유지보수 (NFR-7)
| ID | 요구사항 |
|---|---|
| U2-NFR-EXT1 | 신규 소스는 `Connector` trait 구현만으로 추가(다른 계층 변경 0) — 플러그형. |
| U2-NFR-EXT2 | 스케줄러는 U2가 소유하지 않음(U1 셸이 `trigger` 주기 호출) — 결합도↓. |
| U2-NFR-MNT1 | 마스킹 규칙은 데이터/설정으로 분리해 보강 가능(코드 수정 최소화). |

## 7. 테스트 품질 (PBT — Partial 모드)
| ID | 요구사항 | 규칙 |
|---|---|---|
| U2-NFR-TEST1 | 마스킹 왕복 `unmask(mask(x))==x` 속성 테스트. | PBT-02 |
| U2-NFR-TEST2 | 멱등성·마스킹 잔존 부재·seen 단조성 불변식 테스트. | PBT-03 |
| U2-NFR-TEST3 | 도메인 생성기(식별정보 포함 텍스트, RawItem) 품질 확보. | PBT-07 |
| U2-NFR-TEST4 | 실패 케이스 shrinking + seed 로깅(재현 가능). | PBT-08 |
| U2-NFR-TEST5 | PBT는 example 기반 테스트를 보완(대체 아님). | PBT-10(advisory) |

## 8. 유의사항 (U2 범위 밖으로 위임)
- 저장 암호화 자체(AES-GCM)는 U1 `EncryptedStore` 책임. U2는 그 게이트만 사용.
- 스케줄러 구현은 U1 셸. U2는 `IngestionApi.trigger` 계약만 노출.
