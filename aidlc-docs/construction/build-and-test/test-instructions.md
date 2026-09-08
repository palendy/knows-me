# 테스트 지침 (전 유닛)

## 한 번에 전부
```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd .. && npm test && npx tsc --noEmit
```

## 단위 테스트
```bash
cd src-tauri && cargo test --lib        # 148개 — U1~U4 각 모듈
npm test                                 # 41개 — U4 프론트엔드
```

## 통합 테스트 (유닛 간 실물 연동)
```bash
cd src-tauri && cargo test --test integration_all_units   # 9개
```
U1 실제 KeyManager/EncryptedStore/Masker → U3 실제 Knowledge/Interview → U4 QueryService/PersonaService/LocalApiServer를 실제 디스크 위에서 엮어 검증한다. 네트워크만 테스트 더블.

## 속성 기반 테스트 (PBT)
```bash
cd src-tauri && cargo test --test u2_pbt      # U2 속성 3개
cd src-tauri && cargo test persona::properties # U4 속성
npm test -- property                           # 프론트엔드 속성
```

**실패 재현**:
- Rust(proptest): 실패 시 축소된 최소 케이스가 `src-tauri/proptest-regressions/`에 기록되고 커밋된다. 특정 실행 재현은 `PROPTEST_SEED=<seed> cargo test`.
- TS(fast-check): 실패 시 seed가 출력된다. `fc.assert(prop, { seed })`로 재현.
- shrinking은 어느 쪽도 비활성화하지 않는다. flaky는 억제하지 말고 원인을 찾는다 (PBT-08).

## 보안 점검
```bash
grep -rInE "(api[_-]?key|secret|token|password)[\"' ]*[:=][\"' ]*[A-Za-z0-9_\-]{16,}" src src-tauri/src desktop/src
grep -rn "0\.0\.0\.0\|UNSPECIFIED" src-tauri/src desktop/src    # 로컬 API가 외부에 열리지 않았는지
```

## 마지막 검증 결과 (2026-09-08)
Rust 160 pass · 프론트엔드 41 pass · 실패 0 · clippy/fmt clean.
남은 이슈는 코드 결함이 아니라 **앱 배선 누락** — `integration-verification-report.md` 참조.
