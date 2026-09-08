// US-5.2 — the "미니홈피" one-screen view of the owner.
//
// A 3x3 grid of the most representative confirmed facts, ranked by how
// connected they are (BR-V2). Unconfirmed candidates never appear here — this
// screen is meant to be "what is actually true about me".

import { useCallback, useEffect, useState } from "react";
import type { MiniHomeDto } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { badge, card, muted, scopeLabel } from "../u4-shared/styles";
import { load, loading, type ViewState } from "../u4-shared/view-state";

interface Props {
  api: KnowsMeApi;
  limit?: number;
}

export function MiniHomeView({ api, limit = 9 }: Props) {
  const [state, setState] = useState<ViewState<MiniHomeDto>>(loading);

  const refresh = useCallback(() => {
    setState(loading());
    void load(
      () => api.getMiniHome(limit),
      (d) => d.highlights.length === 0,
    ).then(setState);
  }, [api, limit]);

  useEffect(refresh, [refresh]);

  return (
    <section aria-label="미니홈피">
      <h2>미니홈피</h2>
      <StateShell
        state={state}
        emptyMessage="확정된 사실이 아직 없습니다. 인터뷰 Queue에서 질문에 답하면 이 화면이 채워집니다."
        onRetry={refresh}
      >
        {(d) => (
          <>
            <div style={{ ...card, marginBottom: 12 }}>
              <strong>나</strong>
              <div style={muted}>
                확정된 사실 {d.highlights.length}개로 구성된 프로필
              </div>
            </div>
            <ul
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(3, 1fr)",
                gap: 12,
                listStyle: "none",
                padding: 0,
                margin: 0,
              }}
            >
              {d.highlights.map((f) => (
                <li key={f.id} style={card}>
                  <div style={{ fontWeight: 600 }}>{f.title}</div>
                  <span style={badge(f.scope)}>{scopeLabel(f.scope)}</span>
                </li>
              ))}
            </ul>
          </>
        )}
      </StateShell>
    </section>
  );
}
