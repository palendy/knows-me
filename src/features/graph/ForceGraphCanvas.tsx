// US-5.3 — the Obsidian-style force-directed canvas.
//
// Split out from GraphView so it can be lazy-loaded: react-force-graph-2d pulls
// in d3-force and a live <canvas>, neither of which belongs in the jsdom test
// path. GraphView keeps a plain-DOM node list for accessibility and tests; this
// component is the purely-visual layer on top.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ForceGraph from "react-force-graph-2d";
import type { ForceGraphMethods, NodeObject } from "react-force-graph-2d";
import type { FactId } from "../../shared/contracts";
import type { ForceGraphData, ForceNode } from "../u4-shared/graph-force";

type GNode = NodeObject<ForceNode>;

interface Props {
  data: ForceGraphData;
  selected: FactId | null;
  neighbors: Set<FactId>;
  onSelect: (id: FactId | null) => void;
}

// knows-me light palette (matches --accent #365d4b).
const NODE_IDLE = "#91a491";
const NODE_SELECTED = "#365d4b";
const NODE_NEIGHBOR = "#5f7d64";
const LABEL_INK = "#252923";
const LABEL_BG = "rgba(255,255,255,0.82)";
const LINK_IDLE = "#c7d1c8";
const LINK_ACTIVE = "#365d4b";
const BG = "#fafbf8";

/** Radius grows gently with connection count. Kept small relative to the
 *  simulation's link distance (~48) so nodes read as points, not blobs. */
function nodeRadius(n: ForceNode): number {
  return 3 + Math.min(4, n.degree * 0.8);
}

/** Link endpoints are strings before the sim runs, node objects afterwards. */
function endpointId(val: unknown): FactId | undefined {
  if (val && typeof val === "object" && "id" in val) return (val as GNode).id as FactId;
  if (typeof val === "string") return val;
  return undefined;
}

export default function ForceGraphCanvas({ data, selected, neighbors, onSelect }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fgRef = useRef<ForceGraphMethods<ForceNode, { source: FactId; target: FactId }>>(
    undefined as unknown as ForceGraphMethods<ForceNode, { source: FactId; target: FactId }>,
  );
  const [dims, setDims] = useState({ width: 640, height: 420 });
  const [hovered, setHovered] = useState<FactId | null>(null);

  // Track container size so the canvas fills the panel responsively. We watch
  // the wrapper's *parent* (`.graph-canvas-region`): the wrapper itself is
  // position:absolute/inset:0, whose own box can measure 0×0 on first paint and
  // would hand the simulation a zero-size viewport (nodes collapse to a point).
  useEffect(() => {
    const el = containerRef.current;
    const box = el?.parentElement ?? el;
    if (!box) return;
    const measure = () => {
      const rect = box.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        setDims({ width: rect.width, height: rect.height });
      }
    };
    measure(); // size immediately, before the observer's first async callback
    const ro = new ResizeObserver(measure);
    ro.observe(box);
    return () => ro.disconnect();
  }, []);

  // Tune the physics so the web holds together instead of flying apart: a
  // light repulsion and short links keep neighbours close, and zoomToFit then
  // scales the whole thing to the panel. Retried on a few frames because the
  // simulation object isn't guaranteed to exist on the first effect run.
  useEffect(() => {
    let tries = 0;
    let raf = 0;
    const apply = () => {
      const fg = fgRef.current;
      if (fg?.d3Force) {
        fg.d3Force("charge")?.strength(-45);
        fg.d3Force("link")?.distance(34);
        fg.d3ReheatSimulation();
        return;
      }
      if (tries++ < 30) raf = requestAnimationFrame(apply);
    };
    apply();
    return () => cancelAnimationFrame(raf);
  }, [data]);

  // Fit the whole graph into view. Fire on several frames after data/size
  // changes because the WKWebview canvas can report zero size on the first
  // paint, which would make a single fit no-op. onEngineStop refits on settle.
  useEffect(() => {
    const timers = [200, 600, 1200].map((ms) =>
      setTimeout(() => fgRef.current?.zoomToFit(400, 70), ms),
    );
    return () => timers.forEach(clearTimeout);
  }, [data, dims]);

  const isDimmed = useCallback(
    (id: FactId | undefined) =>
      selected !== null && id !== undefined && id !== selected && !neighbors.has(id),
    [selected, neighbors],
  );

  const drawNode = useCallback(
    (node: GNode, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const x = node.x ?? 0;
      const y = node.y ?? 0;
      const id = node.id as FactId;
      const r = nodeRadius(node);
      const dimmed = isDimmed(id);
      const isSelected = selected === id;
      const isNeighbor = neighbors.has(id);
      const isHovered = hovered === id;

      ctx.globalAlpha = dimmed ? 0.2 : 1;

      // Selection / hover halo.
      if (isSelected || isHovered) {
        ctx.beginPath();
        ctx.arc(x, y, r + 4, 0, 2 * Math.PI);
        ctx.fillStyle = `${NODE_SELECTED}1f`;
        ctx.fill();
      }

      // Node dot.
      ctx.beginPath();
      ctx.arc(x, y, r, 0, 2 * Math.PI);
      ctx.fillStyle = isSelected ? NODE_SELECTED : isNeighbor ? NODE_NEIGHBOR : NODE_IDLE;
      ctx.strokeStyle = isSelected ? "#e0e9df" : "#ffffff";
      ctx.lineWidth = 1.5;
      ctx.fill();
      ctx.stroke();

      // Label (scale-aware, hidden when zoomed far out to reduce clutter).
      const fontSize = Math.min(12 / globalScale, 5);
      if (globalScale > 0.55 || isSelected || isHovered) {
        const raw = node.label ?? "";
        const label = raw.length > 22 ? `${raw.slice(0, 21)}…` : raw;
        ctx.font = `500 ${fontSize}px 'Pretendard Variable', Pretendard, system-ui, sans-serif`;
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        const textW = ctx.measureText(label).width;
        const pad = fontSize * 0.4;
        const lx = x + r + pad;
        // Label chip so text stays legible over links.
        ctx.fillStyle = LABEL_BG;
        ctx.beginPath();
        ctx.roundRect(lx - pad * 0.5, y - fontSize * 0.72, textW + pad, fontSize * 1.44, 2);
        ctx.fill();
        ctx.fillStyle = LABEL_INK;
        ctx.fillText(label, lx, y);
      }

      ctx.globalAlpha = 1;
    },
    [selected, neighbors, hovered, isDimmed],
  );

  // Extend the clickable/hover area to cover the label chip, not just the dot.
  const paintPointerArea = useCallback(
    (node: GNode, color: string, ctx: CanvasRenderingContext2D) => {
      const x = node.x ?? 0;
      const y = node.y ?? 0;
      const r = nodeRadius(node);
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(x, y, r + 2, 0, 2 * Math.PI);
      ctx.fill();
    },
    [],
  );

  const linkColor = useCallback(
    (link: { source?: unknown; target?: unknown }) => {
      const s = endpointId(link.source);
      const t = endpointId(link.target);
      const touchesSelection =
        selected !== null && (s === selected || t === selected);
      const touchesHover = hovered !== null && (s === hovered || t === hovered);
      if (touchesSelection || touchesHover) return LINK_ACTIVE;
      if (selected !== null && isDimmed(s) && isDimmed(t)) return "rgba(199,209,200,0.25)";
      return LINK_IDLE;
    },
    [selected, hovered, isDimmed],
  );

  const graphData = useMemo(
    // react-force-graph mutates node/link objects, so hand it fresh copies.
    () => ({
      nodes: data.nodes.map((n) => ({ ...n })),
      links: data.links.map((l) => ({ ...l })),
    }),
    [data],
  );

  return (
    <div ref={containerRef} className="graph-force-canvas">
      <ForceGraph<ForceNode, { source: FactId; target: FactId }>
        ref={fgRef}
        graphData={graphData}
        width={dims.width}
        height={dims.height}
        backgroundColor={BG}
        nodeCanvasObject={drawNode}
        nodeCanvasObjectMode={() => "replace"}
        nodePointerAreaPaint={paintPointerArea}
        linkColor={linkColor}
        linkWidth={(link) => {
          const s = endpointId(link.source);
          const t = endpointId(link.target);
          return (selected !== null && (s === selected || t === selected)) ||
            (hovered !== null && (s === hovered || t === hovered))
            ? 2
            : 1;
        }}
        linkCurvature={0.08}
        // Physics — gentle floating settle, like Obsidian's graph view.
        d3AlphaDecay={0.02}
        d3VelocityDecay={0.3}
        cooldownTicks={120}
        onEngineStop={() => fgRef.current?.zoomToFit(400, 80)}
        onNodeClick={(node) => {
          const id = node.id as FactId;
          onSelect(selected === id ? null : id);
        }}
        onNodeHover={(node) => setHovered((node?.id as FactId) ?? null)}
        onBackgroundClick={() => onSelect(null)}
        enableNodeDrag={true}
        enableZoomInteraction={true}
        enablePanInteraction={true}
        minZoom={0.4}
        maxZoom={8}
      />
    </div>
  );
}
