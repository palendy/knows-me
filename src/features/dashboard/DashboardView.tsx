// US-5.1 — collection status, pending queue size, recently confirmed facts.
//
// The numbers are U3's (BR-V1): this view displays what `get_dashboard`
// returns and never recomputes them. AC2 ("latest values are reflected") is met
// by pulling a fresh snapshot on mount and on demand, which is enough while
// there is no push channel.

import { useCallback, useEffect, useState } from "react";
import type { DashboardDto } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { badge, card, muted, scopeLabel } from "../u4-shared/styles";
import { load, loading, type ViewState } from "../u4-shared/view-state";

interface Props {
  api: KnowsMeApi;
}

function StatCard({ label, value }: { label: string; value: number }) {
  return (
    <div style={{ ...card, minWidth: 128 }}>
      <div style={muted}>{label}</div>
      <div style={{ fontSize: 28, fontWeight: 600 }}>{value}</div>
    </div>
  );
}

export function DashboardView({ api }: Props) {
  const [state, setState] = useState<ViewState<DashboardDto>>(loading);

  const refresh = useCallback(() => {
    setState(loading());
    void load(
      () => api.getDashboard(),
      (d) =>
        d.collected_count === 0 &&
        d.pending_queue === 0 &&
        d.recent_facts.length === 0,
    ).then(setState);
  }, [api]);

  useEffect(refresh, [refresh]);

  return (
    <section aria-label="대시보드">
      <header style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <h2 style={{ margin: 0 }}>대시보드</h2>
        <button type="button" onClick={refresh}>
          새로고침
        </button>
      </header>

      <StateShell
        state={state}
        emptyMessage="아직 수집된 내용이 없습니다. 소스를 연결하면 여기에 현황이 표시됩니다."
        onRetry={refresh}
      >
        {(d) => (
          <>
            <div style={{ display: "flex", gap: 12, marginTop: 12 }}>
              <StatCard label="수집 현황" value={d.collected_count} />
              <StatCard label="대기 중인 질문" value={d.pending_queue} />
              <StatCard label="최근 확정 사실" value={d.recent_facts.length} />
            </div>

            <h3>최근 확정된 사실</h3>
            {d.recent_facts.length === 0 ? (
              <p style={muted}>확정된 사실이 아직 없습니다.</p>
            ) : (
              <ul>
                {d.recent_facts.map((f) => (
                  <li key={f.id}>
                    {f.title}{" "}
                    <span style={badge(f.scope)}>{scopeLabel(f.scope)}</span>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </StateShell>
    </section>
  );
}
