import { useCallback, useEffect, useState } from "react";
import type { MiniHomeDto } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { kindLabel, scopeLabel, visibilityLabel } from "../u4-shared/styles";
import { load, loading, type ViewState } from "../u4-shared/view-state";
import "../dashboard/dashboard.css";

interface Props { api: KnowsMeApi; limit?: number; }

export function MiniHomeView({ api, limit = 9 }: Props) {
  const [state, setState] = useState<ViewState<MiniHomeDto>>(loading);
  const refresh = useCallback(() => {
    setState(loading());
    void load(() => api.getMiniHome(limit), (d) => d.highlights.length === 0).then(setState);
  }, [api, limit]);
  useEffect(refresh, [refresh]);

  return (
    <section aria-label="나를 이루는 맥락" className="context-section">
      <div className="dashboard-section-heading">
        <div><h3>나를 이루는 맥락</h3><p>연결이 많은 기록부터, 지금의 나를 한눈에.</p></div>
        {state.status === "ready" && <span className="context-count">{state.data.highlights.length}개의 기록</span>}
      </div>
      <StateShell state={state} emptyMessage="확정된 사실이 아직 없습니다. 인터뷰 Queue에서 질문에 답하면 이 화면이 채워집니다." onRetry={refresh}>
        {(d) => (
          <ul className="context-grid">
            {d.highlights.map((f, index) => (
              <li key={f.id} className="context-card">
                <div className="context-card-top">
                  <span className={`context-kind context-kind-${f.kind.toLowerCase()}`}>{kindLabel(f.kind)}</span>
                  <span className={`context-scope context-scope-${f.scope.toLowerCase()}`}>{scopeLabel(f.scope)}</span>
                  <span className="context-card-index" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                </div>
                <h4>{f.title}</h4>
                {f.topics.length > 0 && (
                  <ul className="context-topics" aria-label="주제">
                    {f.topics.map((t) => (
                      <li key={t}>{t}</li>
                    ))}
                  </ul>
                )}
                <div className="context-card-foot">
                  <span className="context-confirmed"><span aria-hidden="true">✓</span> 확인한 사실</span>
                  <span className={`context-visibility context-visibility-${f.visibility.toLowerCase()}`}>{visibilityLabel(f.visibility)}</span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </StateShell>
    </section>
  );
}
