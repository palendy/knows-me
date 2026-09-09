// US-5.3 — the knowledge graph.
//
// Rendered as plain SVG with a deterministic ring layout rather than a physics
// simulation: the same graph must land in the same place every time, or the
// owner loses their spatial memory of it between visits (BR-V4).

import { useCallback, useEffect, useMemo, useState } from "react";
import type { FactId, GraphDto, Scope } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { muted } from "../u4-shared/styles";
import "../u4-shared/work-views.css";
import {
  layoutGraph,
  neighborsOf,
  normalizeGraph,
} from "../u4-shared/graph-layout";
import { load, loading, type ViewState } from "../u4-shared/view-state";

interface Props {
  api: KnowsMeApi;
  width?: number;
  height?: number;
  embedded?: boolean;
}

const SCOPES: Array<{ value: Scope | ""; label: string }> = [
  { value: "", label: "전체" },
  { value: "Company", label: "업무" },
  { value: "Personal", label: "개인" },
  { value: "Unknown", label: "미분류" },
];

export function GraphView({ api, width = 640, height = 480, embedded = false }: Props) {
  const [state, setState] = useState<ViewState<GraphDto>>(loading);
  const [scope, setScope] = useState<Scope | "">("");
  const [selected, setSelected] = useState<FactId | null>(null);

  const refresh = useCallback(() => {
    setState(loading());
    setSelected(null);
    void load(
      () => api.getGraph({ scope: scope === "" ? null : scope }),
      (g) => g.nodes.length === 0,
    ).then(setState);
  }, [api, scope]);

  useEffect(refresh, [refresh]);

  const normalized = useMemo(
    () => (state.status === "ready" ? normalizeGraph(state.data) : null),
    [state],
  );
  const layout = useMemo(
    () => (normalized ? layoutGraph(normalized, width, height) : null),
    [normalized, width, height],
  );
  const neighbors = useMemo(
    () =>
      normalized && selected ? neighborsOf(normalized, selected) : new Set<FactId>(),
    [normalized, selected],
  );

  const isDimmed = (id: FactId) =>
    selected !== null && id !== selected && !neighbors.has(id);

  const toggle = (id: FactId) => setSelected((cur) => (cur === id ? null : id));

  return (
    <section aria-label="지식 그래프" className={`graph-workspace${embedded ? " graph-workspace--embedded" : ""}`}>
      <header className="work-view-header">
        <div>{embedded ? <h3>지식 그래프</h3> : <h2>지식 그래프</h2>}<p>따로 쌓인 기록이 어떻게 이어지는지 살펴보세요.</p></div>
        <label>
          범위{" "}
          <select
            value={scope}
            onChange={(e) => setScope(e.target.value as Scope | "")}
          >
            {SCOPES.map((s) => (
              <option key={s.label} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
      </header>

      <StateShell
        state={state}
        emptyMessage="표시할 사실이 없습니다. 사실이 확정되고 서로 연결되면 그래프가 나타납니다."
        onRetry={refresh}
      >
        {() =>
          layout === null ? null : (
            <div className="graph-panel">
              <div className="graph-toolbar"><span>내 맥락의 연결</span><span>사실 <strong>{layout.nodes.length}</strong> · 연결 <strong>{layout.edges.length}</strong></span></div>
              {normalized?.truncated && (
                <p role="status" style={muted}>
                  노드가 많아 연결이 많은 상위 항목만 표시하고 있습니다.
                </p>
              )}
              <div className="graph-explorer"><svg
                viewBox={`0 0 ${layout.width} ${layout.height}`}
                width="100%"
                role="img"
                aria-label={`사실 ${layout.nodes.length}개, 연결 ${layout.edges.length}개`}
                onClick={() => setSelected(null)}
                className="graph-canvas"
              >
                {layout.edges.map((e) => (
                  <line
                    key={`${e.from}-${e.to}`}
                    x1={e.x1}
                    y1={e.y1}
                    x2={e.x2}
                    y2={e.y2}
                    stroke="#c7d1c8"
                    strokeWidth={1}
                    opacity={isDimmed(e.from) && isDimmed(e.to) ? 0.15 : 1}
                  />
                ))}
                {layout.nodes.map((n) => (
                  <g
                    key={n.id}
                    role="button"
                    tabIndex={0}
                    aria-label={n.label}
                    aria-pressed={selected === n.id}
                    opacity={isDimmed(n.id) ? 0.2 : 1}
                    style={{ cursor: "pointer" }}
                    onClick={(ev) => {
                      ev.stopPropagation();
                      toggle(n.id);
                    }}
                    onKeyDown={(ev) => {
                      if (ev.key === "Enter" || ev.key === " ") {
                        ev.preventDefault();
                        toggle(n.id);
                      }
                    }}
                  >
                    <circle
                      cx={n.x}
                      cy={n.y}
                      r={6 + Math.min(6, n.degree)}
                      fill={selected === n.id ? "#365d4b" : "#91a491"}
                      stroke={selected === n.id ? "#e0e9df" : "#fff"}
                      strokeWidth={4}
                    />
                    <text x={n.x > layout.width * 0.7 ? n.x - 16 : n.x + 16} textAnchor={n.x > layout.width * 0.7 ? "end" : "start"} y={n.y + 4} fontSize={11} fill="#252923">
                      {n.label.length > 22 ? `${n.label.slice(0, 21)}…` : n.label}
                    </text>
                  </g>
                ))}
              </svg><aside className="graph-inspector">
                <span className="work-eyebrow">{selected ? "선택한 사실" : "연결 살펴보기"}</span>
                <h3>{selected ? layout.nodes.find((n) => n.id === selected)?.label : "기록 사이의 관계"}</h3>
                {selected ? <><p>직접 연결된 사실 {neighbors.size}개</p><ul>{layout.nodes.filter((n) => neighbors.has(n.id)).map((n) => <li key={n.id}><button type="button" onClick={() => toggle(n.id)}>{n.label}<span aria-hidden="true">↗</span></button></li>)}</ul></> : <p>점을 선택하면 연결된 사실을 함께 볼 수 있어요. 연결이 많은 사실일수록 중심에 가깝게 표시됩니다.</p>}
              </aside></div>

              {selected !== null && (
                <p role="status" className="graph-status">
                  선택한 사실과 직접 연결된 항목 {neighbors.size}개를 강조하고
                  있습니다. 배경을 클릭하면 해제됩니다.
                </p>
              )}
            </div>
          )
        }
      </StateShell>
    </section>
  );
}
