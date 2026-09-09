import { useCallback, useEffect, useState } from "react";
import type { DashboardDto } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { scopeLabel } from "../u4-shared/styles";
import { load, loading, type ViewState } from "../u4-shared/view-state";
import { MiniHomeView } from "../minihome/MiniHomeView";
import { GraphView } from "../graph/GraphView";
import "./dashboard.css";

interface Props {
  api: KnowsMeApi;
  onNavigate?: (view: "queue" | "persona" | "settings") => void;
}

function StatCard({ label, value, detail }: { label: string; value: number; detail: string }) {
  return (
    <div className="dashboard-stat" role="group" aria-label={label}>
      <div className="dashboard-stat-label">{label}</div>
      <div className="dashboard-stat-value">{value.toLocaleString("ko-KR")}<span>개</span></div>
      <p>{detail}</p>
    </div>
  );
}

export function DashboardView({ api, onNavigate }: Props) {
  const [state, setState] = useState<ViewState<DashboardDto>>(loading);
  const refresh = useCallback(() => {
    setState(loading());
    void load(
      () => api.getDashboard(),
      (d) => d.collected_count === 0 && d.pending_queue === 0 && d.recent_facts.length === 0,
    ).then(setState);
  }, [api]);
  useEffect(refresh, [refresh]);

  return (
    <section aria-label="대시보드" className="dashboard-view">
      <header className="dashboard-heading">
        <div>
          <p className="dashboard-eyebrow">MY CONTEXT</p>
          <h2>대시보드</h2>
          <p className="dashboard-intro">흩어진 기록이 모여, 나를 조금 더 선명하게.</p>
        </div>
        <button type="button" className="dashboard-refresh" onClick={refresh}>
          <span aria-hidden="true">↻</span> 새로고침
        </button>
      </header>
      <StateShell state={state} emptyMessage="아직 수집된 내용이 없습니다. 소스를 연결하면 여기에 현황이 표시됩니다." onRetry={refresh}>
        {(d) => (
          <>
            <div className="dashboard-stats">
              <StatCard label="수집 현황" value={d.collected_count} detail="연결한 소스에서 모은 기록" />
              <StatCard label="대기 중인 질문" value={d.pending_queue} detail="내 확인을 기다리는 이야기" />
              <StatCard label="최근 확정 사실" value={d.recent_facts.length} detail="최근에 확인한 나의 맥락" />
            </div>
            <div className="dashboard-body">
              <div className="dashboard-main">
                <MiniHomeView api={api} />
                <section className="dashboard-recent" aria-label="최근 확정된 사실">
                  <div className="dashboard-section-heading">
                    <div><h3>최근 확정된 사실</h3><p>직접 확인한 기록이 차곡차곡 쌓이고 있어요.</p></div>
                    <a className="dashboard-text-link" href="#dashboard-graph">연결 살펴보기 <span aria-hidden="true">↓</span></a>
                  </div>
                  {d.recent_facts.length === 0 ? <p className="dashboard-empty">확정된 사실이 아직 없습니다.</p> : (
                    <ul className="dashboard-facts">
                      {d.recent_facts.map((f) => (
                        <li key={f.id}><span className="dashboard-fact-dot" aria-hidden="true" /><span className="dashboard-fact-title">{f.title}</span><span className={`context-scope context-scope-${f.scope.toLowerCase()}`}>{scopeLabel(f.scope)}</span></li>
                      ))}
                    </ul>
                  )}
                </section>
              </div>
              <aside className="dashboard-aside" aria-label="다음 활동">
                <section className="dashboard-composition" aria-label="최근 맥락의 구성">
                  <span className="dashboard-aside-label">기록의 관점</span>
                  <h3>최근 맥락의 구성</h3>
                  <p>최근 확정 사실 {d.recent_facts.length}개 기준</p>
                  <div className="dashboard-scope-bars">
                    {(["Company", "Personal", "Unknown"] as const).map((scope) => {
                      const count = d.recent_facts.filter((fact) => fact.scope === scope).length;
                      return <div className="dashboard-scope-row" key={scope}>
                        <div><span>{scopeLabel(scope)}</span><strong>{count}개</strong></div>
                        <div className="dashboard-scope-track"><span className={`dashboard-scope-fill dashboard-scope-fill-${scope.toLowerCase()}`} style={{ width: `${d.recent_facts.length ? count / d.recent_facts.length * 100 : 0}%` }} /></div>
                      </div>;
                    })}
                  </div>
                </section>
                <div className="dashboard-next">
                  <span className="dashboard-aside-label">이어서 할 일</span>
                  <h3>{d.pending_queue > 0 ? "한 가지 더 알려주세요" : "나의 기록을 넓혀보세요"}</h3>
                  <p>{d.pending_queue > 0 ? `확인을 기다리는 질문 ${d.pending_queue}개가 있어요. 짧은 답변 하나로 나의 맥락이 더 정확해집니다.` : "새로운 소스를 연결하면 일상과 업무의 기록을 한곳에서 살펴볼 수 있어요."}</p>
                  {onNavigate && <button className="dashboard-primary" onClick={() => onNavigate(d.pending_queue > 0 ? "queue" : "settings")}>{d.pending_queue > 0 ? "질문 확인하기" : "소스 연결하기"}<span aria-hidden="true">→</span></button>}
                </div>
                <div className="dashboard-conversation">
                  <span className="dashboard-aside-label">나와 대화</span>
                  <h3>기록에 말을 걸어보세요</h3>
                  <p>쌓인 맥락을 바탕으로 생각을 정리하고, 나다운 문장을 만들어보세요.</p>
                  {onNavigate && <button className="dashboard-text-link" onClick={() => onNavigate("persona")}>대화 시작하기 <span aria-hidden="true">→</span></button>}
                </div>
              </aside>
            </div>
            <div id="dashboard-graph" className="dashboard-graph"><GraphView api={api} embedded /></div>
          </>
        )}
      </StateShell>
      {state.status === "empty" && onNavigate && <button className="dashboard-primary" onClick={() => onNavigate("settings")}>소스 연결하기 <span aria-hidden="true">→</span></button>}
    </section>
  );
}
