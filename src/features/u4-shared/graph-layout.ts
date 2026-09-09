// Knowledge-graph normalization and layout (US-5.3).
//
// Deliberately dependency-free and deterministic: a force simulation would
// place the same graph differently on every render, which breaks both the
// owner's spatial memory and any hope of testing the output (BR-V4).
// Concentric rings ordered by connection degree give a stable picture where
// the most-linked facts sit at the centre.

import type { FactId, GraphDto, GraphEdge, GraphNode } from "../../shared/contracts";

/** Above this many nodes the view truncates rather than crawl (U4-NFR-P4). */
export const MAX_NODES = 500;

export interface NormalizedGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** True when node truncation dropped part of the graph. */
  truncated: boolean;
}

export interface PositionedNode {
  id: FactId;
  label: string;
  x: number;
  y: number;
  degree: number;
}

export interface LayoutEdge {
  from: FactId;
  to: FactId;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface GraphLayout {
  nodes: PositionedNode[];
  edges: LayoutEdge[];
  width: number;
  height: number;
}

function degreeMap(edges: readonly GraphEdge[]): Map<FactId, number> {
  const deg = new Map<FactId, number>();
  for (const e of edges) {
    deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
    deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
  }
  return deg;
}

/**
 * Drop self-loops, collapse duplicates, remove edges whose endpoints are not
 * both present, and cap the node count (BR-V3, U4-NFR-P4).
 *
 * Cleanup comes first and truncation second: ranking nodes by a degree counted
 * over raw edges would let self-loops and duplicates inflate a node past one
 * that is genuinely better connected. After truncation the edges are filtered
 * again against the surviving node set, so truncation cannot leave dangling
 * edges behind — which is the thing this function exists to prevent.
 */
export function normalizeGraph(g: GraphDto, maxNodes: number = MAX_NODES): NormalizedGraph {
  const uniqueNodes: GraphNode[] = [];
  const seenNodes = new Set<FactId>();
  for (const n of g.nodes) {
    if (seenNodes.has(n.id)) continue;
    seenNodes.add(n.id);
    uniqueNodes.push(n);
  }
  const allIds = new Set(uniqueNodes.map((n) => n.id));

  // Pass 1 — clean the edges against the full node set.
  const seenEdges = new Set<string>();
  const cleaned: GraphEdge[] = [];
  for (const e of g.edges) {
    if (e.from === e.to) continue;
    if (!allIds.has(e.from) || !allIds.has(e.to)) continue;
    // Undirected for display: A->B and B->A are one line.
    const key = e.from < e.to ? `${e.from}|${e.to}` : `${e.to}|${e.from}`;
    if (seenEdges.has(key)) continue;
    seenEdges.add(key);
    cleaned.push(e);
  }

  // Pass 2 — rank by real connection degree, then cap.
  const deg = degreeMap(cleaned);
  const truncated = uniqueNodes.length > maxNodes;
  const kept = [...uniqueNodes]
    .sort((a, b) => {
      const byDegree = (deg.get(b.id) ?? 0) - (deg.get(a.id) ?? 0);
      if (byDegree !== 0) return byDegree;
      return a.id.localeCompare(b.id);
    })
    .slice(0, Math.max(0, maxNodes));

  // Pass 3 — re-check the cleaned edges against what survived the cap.
  const present = new Set(kept.map((n) => n.id));
  const edges = cleaned.filter((e) => present.has(e.from) && present.has(e.to));

  return { nodes: kept, edges, truncated };
}

/**
 * Place nodes on concentric rings, most-connected first.
 *
 * Ring `k` holds up to `6k` nodes at radius `k * step`, which keeps spacing
 * roughly even as the graph grows. No randomness and no time input, so the
 * same graph always lands in the same place (BR-V4); every coordinate is
 * clamped inside the viewbox (BR-V5).
 */
export function layoutGraph(
  g: NormalizedGraph,
  width: number,
  height: number,
): GraphLayout {
  const cx = width / 2;
  const cy = height / 2;
  const margin = 28;
  const maxRadius = Math.max(0, Math.min(cx, cy) - margin);

  const deg = degreeMap(g.edges);
  const ordered = [...g.nodes].sort((a, b) => {
    const byDegree = (deg.get(b.id) ?? 0) - (deg.get(a.id) ?? 0);
    if (byDegree !== 0) return byDegree;
    return a.id.localeCompare(b.id);
  });

  // Ring capacities: 1, 6, 12, 18, ... — enough rings for every node.
  const rings: GraphNode[][] = [];
  let index = 0;
  let ring = 0;
  while (index < ordered.length) {
    const capacity = ring === 0 ? 1 : 6 * ring;
    rings.push(ordered.slice(index, index + capacity));
    index += capacity;
    ring += 1;
  }
  const ringCount = rings.length;
  const step = ringCount > 1 ? maxRadius / (ringCount - 1) : 0;

  const positions = new Map<FactId, PositionedNode>();
  const clamp = (v: number, lo: number, hi: number) =>
    Math.min(hi, Math.max(lo, v));

  rings.forEach((members, r) => {
    const radius = r === 0 ? 0 : step * r;
    members.forEach((n, i) => {
      const angle = members.length === 0 ? 0 : (2 * Math.PI * i) / members.length;
      positions.set(n.id, {
        id: n.id,
        label: n.label,
        x: clamp(cx + radius * Math.cos(angle), 0, width),
        y: clamp(cy + radius * Math.sin(angle), 0, height),
        degree: deg.get(n.id) ?? 0,
      });
    });
  });

  const edges: LayoutEdge[] = [];
  for (const e of g.edges) {
    const a = positions.get(e.from);
    const b = positions.get(e.to);
    // normalizeGraph guarantees both exist; the check keeps layoutGraph safe
    // if it is ever called on an un-normalized graph.
    if (!a || !b) continue;
    edges.push({ from: e.from, to: e.to, x1: a.x, y1: a.y, x2: b.x, y2: b.y });
  }

  return {
    nodes: [...positions.values()],
    edges,
    width,
    height,
  };
}

/** Ids directly linked to `id`, used to highlight a selection (US-5.3). */
export function neighborsOf(g: NormalizedGraph, id: FactId): Set<FactId> {
  const out = new Set<FactId>();
  for (const e of g.edges) {
    if (e.from === id) out.add(e.to);
    if (e.to === id) out.add(e.from);
  }
  return out;
}
