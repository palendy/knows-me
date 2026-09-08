# U2 Business Rules — Ingestion & Processing

> 결정 규칙·검증 로직·제약. 각 규칙은 스토리 수용 기준(AC)과 요구사항(FR/NFR)에 추적된다.

## 1. 수집 멱등·증분 규칙 (US-1.1, US-1.5, NFR-5)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-I1 | 중복 판정 키는 `(SourceKind, external_id)`. 이미 seen이면 스킵(skipped++), 신규만 처리. | Q1=A, US-1.1 AC2 |
| BR-I2 | 신규 항목 처리 성공 후에만 `mark_seen`. 처리 전 실패 시 seen 미기록(다음 주기 재시도 가능). | US-1.1 AC3 |
| BR-I3 | 커서는 소스별 1개. sync 성공 시에만 `save_cursor`(부분 실패 시 커서 미전진). | NFR-5 |
| BR-I4 | 소스 접근 실패는 오류로 기록하고 다음 소스/다음 주기로 계속. 앱 중단 금지. | US-1.1 AC3, NFR-3 |
| BR-I5 | `trigger` 재실행 시 변경 없으면 신규 0(멱등). PBT-03로 검증. | US-1.1 AC2, US-1.3 AC3 |

## 2. 수동 트리거·동시성 규칙 (US-1.2)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-M1 | "지금 수집"은 배치와 **동일한** 증분 수집 경로를 1회 수행. | US-1.2 AC1 |
| BR-M2 | 소스별 run-lock. 진행 중 재트리거 시 중복 실행하지 않고 "진행 중" 상태를 반환. | US-1.2 AC2 |

## 3. 외부 연동·자격증명 규칙 (US-1.3, US-1.4, FR-5)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-C1 | "내 것" 필터: Gmail=내 주소 기준 보낸/받은, Notion=내가 소유/편집자. | Q3=A, US-1.3/1.4 AC1 |
| BR-C2 | 자격증명은 `CredentialStore`(U1 Vault)에만 암호화 저장. 코드/파일 평문 금지. | US-1.3 AC2, FR-5.4, 심사기준⑥ |
| BR-C3 | 인증 없음/만료 시 수집 중단 대신 재인증 유도 신호(AppError) 반환, 앱 계속. | US-1.3 AC2, NFR-3 |
| BR-C4 | Gmail 본문 등 민감 가능 텍스트는 가공 시 반드시 마스킹 대상으로 표시. | US-1.4 AC2, US-2.2 |

## 4. 파일 형식 규칙 (US-1.5)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-F1 | 지원: 텍스트(.txt/.md), 오피스(.docx/.pdf)=본문 파싱, 이미지(.png/.jpg)=비전 추출. | Q7=A, US-1.5 AC1 |
| BR-F2 | 미지원 형식은 건너뛰고 `FileSkipRecord`에 사유 기록. | US-1.5 AC3 |
| BR-F3 | 감시(자동)·수동 업로드는 동일 가공 파이프라인으로 수렴. | US-1.5 AC2 |

## 5. 가공·라우팅 규칙 (US-2.1)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-P1 | 원본을 그대로 저장하지 않는다. 요약·분류 결과(사실 후보)만 하류로 전달. | FR-2.1, US-2.1 AC1 |
| BR-P2 | 라우팅: 잡음/일회성=Drop(filtered++), 불확실=Queue(Confirm/Deepen), 확실=Fact(upsert). | Q4=A, US-2.1 AC1/2/3 |
| BR-P3 | 판정은 `LlmClient.classify` 라벨 + U2 임계 규칙 조합. 라벨 부재 시 보수적으로 Confirm(Queue)로. | US-2.1 AC3 |
| BR-P4 | 확실로 저장되는 Fact의 `scope`는 분류의 company/personal 신호로 설정, 불명확 시 `Unknown`. | FR-2.1 |

## 6. 마스킹·프라이버시 규칙 (US-2.2, NFR-2, R1) — 핵심 보안 규칙
| ID | 규칙 | 추적 |
|---|---|---|
| BR-K1 | 외부 LLM 호출 전 텍스트는 반드시 `Masker.mask`를 통과한 `MaskedText`만 전송. 원문 직접 전송 금지. | US-2.2 AC1, D2 |
| BR-K2 | `UnmaskMap`은 가공 처리 스코프의 메모리에만 존재, 처리 종료 시 폐기. 어떤 저장소에도 기록 금지. 기기 밖 전송 절대 금지. | Q5=A, US-2.2 AC2 |
| BR-K3 | 복원(`unmask`)은 로컬에서만 수행. 복원된 원문은 로컬 저장(Fact body)에만 사용. | US-2.2 AC2 |
| BR-K4 | 마스킹 미검출 패턴은 로깅하여 규칙 보강 가능하게 함(원문 로깅 금지, 패턴/위치만). | US-2.2 AC3, R1 |
| BR-K5 | **불변식**: 마스킹 출력과 TransferLog에 원본 식별정보(이름·이메일·전화·계정·토큰)가 남지 않는다. PBT-03로 검증. | US-2.2, R1 |
| BR-K6 | **왕복 불변식**: `unmask(mask(x)) == x`. PBT-02로 검증. | US-2.2 AC2 |

## 7. 전송 투명성 규칙 (US-2.3, NFR-2)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-T1 | 모든 외부 LLM 호출마다 `TransferLogEntry`를 append(시각·출처·연산·마스킹 후 미리보기·대상). | US-2.3 AC1 |
| BR-T2 | `masked_preview`는 마스킹 후 텍스트에서만 파생. 원문·UnmaskMap 값 기록 금지. | BR-K5 |
| BR-T3 | 로그는 append-only(`EncryptedStore` ns="transfer.log"). 사후 편집·삭제로 흔적 은폐 금지. | NFR-2 |

## 8. 오프라인 저하 규칙 (NFR-3)
| ID | 규칙 | 추적 |
|---|---|---|
| BR-O1 | LLM 미연결 시 수집·원본 대기는 계속 동작. 가공만 `PendingProcessingItem`으로 큐잉. | Q8=A, NFR-3 |
| BR-O2 | 온라인 복귀 시 pending 큐를 정상 가공 경로로 재투입. | Q8=A, US-2.1 |
| BR-O3 | 오프라인은 오류(errors)가 아니라 지연(pending)으로 카운트. | NFR-3 |

## 9. 오류 처리 요약
| 상황 | 동작 |
|---|---|
| 소스 접근 실패 | errors++ 기록, 다음 소스/주기 계속 (BR-I4) |
| 인증 만료 | 재인증 유도 신호, 앱 계속 (BR-C3) |
| 미지원 파일 | skip + 사유 기록 (BR-F2) |
| LLM 오프라인 | pending 큐잉, 재개 (BR-O1/O2) |
| 분류 라벨 부재 | 보수적으로 Queue(Confirm) (BR-P3) |

## 10. 추적성 매트릭스 (규칙 ↔ 스토리/요구사항)
| 스토리 | 관련 규칙 |
|---|---|
| US-1.1 | BR-I1~I5 |
| US-1.2 | BR-M1, BR-M2 |
| US-1.3 | BR-C1, BR-C2, BR-C3, BR-I5 |
| US-1.4 | BR-C1, BR-C4 |
| US-1.5 | BR-F1, BR-F2, BR-F3 |
| US-2.1 | BR-P1~P4, BR-O2 |
| US-2.2 | BR-K1~K6 |
| US-2.3 | BR-T1~T3 |
