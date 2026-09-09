// Force-graph data shaping (US-5.3).
//
// The knowledge graph is now drawn with a d3-force physics simulation
// (react-force-graph-2d) so it floats and settles like an Obsidian graph.
// This module turns the wire `GraphDto` into the {nodes, links} shape the
// library wants, reusing `normalizeGraph` for the cleanup/cap invariants so
// self-loops, duplicates, and dangling edges never reach the simulation.

import type { FactId, GraphDto } from "../../shared/contracts";
import { normalizeGraph, type NormalizedGraph } from "./graph-layout";

/** A node handed to the force simulation. `x`/`y`/`vx`/`vy` are added by d3. */
export interface ForceNode {
  id: FactId;
  label: string;
  /** Connection count, used to size the node. */
  degree: number;
}

/** An undirected link between two facts. */
export interface ForceLink {
  source: FactId;
  target: FactId;
}

export interface ForceGraphData {
  nodes: ForceNode[];
  links: ForceLink[];
  /** True when node truncation dropped part of the graph. */
  truncated: boolean;
}

function degreeMap(g: NormalizedGraph): Map<FactId, number> {
  const deg = new Map<FactId, number>();
  for (const e of g.edges) {
    deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
    deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
  }
  return deg;
}

/**
 * Shape a `GraphDto` into force-simulation input.
 *
 * Cleanup (dedupe, self-loop removal, node cap) is delegated to
 * `normalizeGraph`; here we only compute degrees and rename `from`/`to` to the
 * `source`/`target` keys d3-force expects.
 */
export function toForceGraph(dto: GraphDto): ForceGraphData {
  const normalized = normalizeGraph(dto);
  const deg = degreeMap(normalized);
  return {
    nodes: normalized.nodes.map((n) => ({
      id: n.id,
      label: n.label,
      degree: deg.get(n.id) ?? 0,
    })),
    links: normalized.edges.map((e) => ({ source: e.from, target: e.to })),
    truncated: normalized.truncated,
  };
}

/** Ids directly linked to `id`, used to highlight a selection (US-5.3). */
export function neighborsOf(g: ForceGraphData, id: FactId): Set<FactId> {
  const out = new Set<FactId>();
  for (const l of g.links) {
    if (l.source === id) out.add(l.target);
    if (l.target === id) out.add(l.source);
  }
  return out;
}
