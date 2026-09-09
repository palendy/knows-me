// Knowledge-graph normalization (US-5.3).
//
// The visual layout is now a d3-force simulation (see `graph-force.ts` and
// `ForceGraphCanvas.tsx`), but the cleanup invariants that protect the
// simulation from bad input still live here: this module deduplicates edges,
// drops self-loops, and caps the node count. It is deliberately
// dependency-free so it can be exercised by property tests.

import type { FactId, GraphDto, GraphEdge, GraphNode } from "../../shared/contracts";

/** Above this many nodes the view truncates rather than crawl (U4-NFR-P4). */
export const MAX_NODES = 500;

export interface NormalizedGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** True when node truncation dropped part of the graph. */
  truncated: boolean;
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
 * both present, cap the node count (BR-V3, U4-NFR-P4), and finally drop topic
 * hubs the cap stranded below their ≥2-member floor.
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

  // Pass 3 — drop topic hubs the cap stranded below the backend's ≥2-member
  // floor. Truncation can cut a hub's members while keeping the hub (hubs rank
  // high by degree), leaving exactly the lone pendant that floor exists to
  // prevent. Count each kept node's surviving connections; remove any topic
  // node left with fewer than two.
  const keptSet = new Set(kept.map((n) => n.id));
  const survivingDegree = new Map<FactId, number>();
  for (const e of cleaned) {
    if (keptSet.has(e.from) && keptSet.has(e.to)) {
      survivingDegree.set(e.from, (survivingDegree.get(e.from) ?? 0) + 1);
      survivingDegree.set(e.to, (survivingDegree.get(e.to) ?? 0) + 1);
    }
  }
  const nodes = kept.filter(
    (n) => n.kind !== "topic" || (survivingDegree.get(n.id) ?? 0) >= 2,
  );

  // Pass 4 — re-check the cleaned edges against what survived cap + prune.
  const present = new Set(nodes.map((n) => n.id));
  const edges = cleaned.filter((e) => present.has(e.from) && present.has(e.to));

  return { nodes, edges, truncated };
}
