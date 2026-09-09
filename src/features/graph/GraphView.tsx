// US-5.3 — the knowledge graph.
//
// Rendered as an Obsidian-style force-directed canvas (react-force-graph-2d):
// nodes float on a d3-force simulation and settle so the picture reads as a web
// of connections you can pan, zoom, and drag. The canvas has no accessibility
// tree, so a visually-hidden list of node buttons carries keyboard/SR support
// and is what the tests drive.

import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import type { FactId, GraphDto, Scope } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { StateShell } from "../u4-shared/StateShell";
import { muted } from "../u4-shared/styles";
import "../u4-shared/work-views.css";
import { neighborsOf, toForceGraph } from "../u4-shared/graph-force";
import { load, loading, type ViewState } from "../u4-shared/view-state";

const ForceGraphCanvas = lazy(() => import("./ForceGraphCanvas"));

interface Props {
  api: KnowsMeApi;
  embedded?: boolean;
}

const SCOPES: Array<{ value: Scope | ""; label: string }> = [
  { value: "", label: "전체" },
  { value: "Company", label: "업무" },
  { value: "Personal", label: "개인" },
  { value: "Unknown", label: "미분류" },
];

/**
 * A live <canvas> 2D context is unavailable under jsdom, so react-force-graph
 * throws on render there. Detect it and skip the canvas in tests; the hidden
 * accessibility list still renders and is what the suite asserts against.
 */
const canRenderCanvas =
  typeof document !== "undefined" &&
  typeof HTMLCanvasElement !== "undefined" &&
  typeof HTMLCanvasElement.prototype.getContext === "function" &&
  document.createElement("canvas").getContext("2d") !== null;

export function GraphView({ api, embedded = false }: Props) {
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

  const graph = useMemo(
    () => (state.status === "ready" ? toForceGraph(state.data) : null),
    [state],
  );
  const neighbors = useMemo(
    () => (graph && selected ? neighborsOf(graph, selected) : new Set<FactId>()),
    [graph, selected],
  );

  const toggle = (id: FactId) => setSelected((cur) => (cur === id ? null : id));
  const selectedNode = graph?.nodes.find((n) => n.id === selected);
  const factCount = graph?.nodes.filter((n) => n.kind === "fact").length ?? 0;
  const topicCount = (graph?.nodes.length ?? 0) - factCount;

  return (
    <section
      aria-label="지식 그래프"
      className={`graph-workspace${embedded ? " graph-workspace--embedded" : ""}`}
    >
      <header className="work-view-header">
        <div>
          {embedded ? <h3>지식 그래프</h3> : <h2>지식 그래프</h2>}
          <p>따로 쌓인 기록이 어떻게 이어지는지 살펴보세요.</p>
        </div>
        <label>
          범위{" "}
          <select value={scope} onChange={(e) => setScope(e.target.value as Scope | "")}>
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
          graph === null ? null : (
            <div className="graph-panel">
              <div className="graph-toolbar">
                <span>내 맥락의 연결</span>
                <span>
                  사실 <strong>{factCount}</strong>
                  {topicCount > 0 && (
                    <>
                      {" "}
                      · 주제 <strong>{topicCount}</strong>
                    </>
                  )}{" "}
                  · 연결 <strong>{graph.links.length}</strong>
                </span>
              </div>
              {graph.truncated && (
                <p role="status" style={muted}>
                  노드가 많아 연결이 많은 상위 항목만 표시하고 있습니다.
                </p>
              )}

              <div className="graph-explorer">
                <div
                  className="graph-canvas-region"
                  role="img"
                  aria-label={`사실 ${factCount}개, 연결 ${graph.links.length}개${
                    topicCount > 0 ? `, 주제 ${topicCount}개` : ""
                  }`}
                >
                  {canRenderCanvas && (
                    <Suspense fallback={<div className="graph-canvas-loading" />}>
                      <ForceGraphCanvas
                        data={graph}
                        selected={selected}
                        neighbors={neighbors}
                        onSelect={setSelected}
                      />
                    </Suspense>
                  )}

                  {/* Visually-hidden but focusable node list: keyboard + screen
                      reader access to a canvas that exposes no a11y tree. */}
                  <ul className="graph-node-list">
                    {graph.nodes.map((n) => (
                      <li key={n.id}>
                        <button
                          type="button"
                          aria-pressed={selected === n.id}
                          onClick={() => toggle(n.id)}
                        >
                          {n.label}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>

                <aside className="graph-inspector">
                  <span className="work-eyebrow">
                    {selected
                      ? selectedNode?.kind === "topic"
                        ? "선택한 주제"
                        : "선택한 사실"
                      : "연결 살펴보기"}
                  </span>
                  <h3>{selected ? selectedNode?.label : "기록 사이의 관계"}</h3>
                  {selected ? (
                    <>
                      <p>
                        {selectedNode?.kind === "topic"
                          ? `이 주제에 연결된 사실 ${neighbors.size}개`
                          : `연결된 항목 ${neighbors.size}개`}
                      </p>
                      <ul>
                        {graph.nodes
                          .filter((n) => neighbors.has(n.id))
                          .map((n) => (
                            <li key={n.id}>
                              <button type="button" onClick={() => toggle(n.id)}>
                                {n.label}
                                <span aria-hidden="true">↗</span>
                              </button>
                            </li>
                          ))}
                      </ul>
                    </>
                  ) : (
                    <p>
                      점을 선택하면 연결된 사실을 함께 볼 수 있어요. 그래프는
                      드래그·확대·이동할 수 있습니다.
                    </p>
                  )}
                </aside>
              </div>

              {selected !== null && (
                <p role="status" className="graph-status">
                  {selectedNode?.kind === "topic" ? "선택한 주제와" : "선택한 사실과"}{" "}
                  직접 연결된 항목 {neighbors.size}개를 강조하고 있습니다. 배경을
                  클릭하면 해제됩니다.
                </p>
              )}
            </div>
          )
        }
      </StateShell>
    </section>
  );
}
