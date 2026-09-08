// Property-based tests for graph normalization and layout (PBT-03).
//
// On failure fast-check prints the shrunk counterexample and its seed; replay
// it with `fc.assert(prop, { seed: <seed> })` (PBT-08).

import { describe, expect, it } from "vitest";
import fc from "fast-check";
import { layoutGraph, normalizeGraph, neighborsOf } from "./graph-layout";
import { arbGraph } from "./testgen";

describe("normalizeGraph", () => {
  it("keeps only edges whose endpoints both survive", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        const present = new Set(n.nodes.map((x) => x.id));
        for (const e of n.edges) {
          expect(present.has(e.from)).toBe(true);
          expect(present.has(e.to)).toBe(true);
        }
      }),
    );
  });

  it("collapses duplicate and reversed edges and drops self-loops", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        const keys = n.edges.map((e) =>
          e.from < e.to ? `${e.from}|${e.to}` : `${e.to}|${e.from}`,
        );
        expect(new Set(keys).size).toBe(keys.length);
        for (const e of n.edges) expect(e.from).not.toBe(e.to);
      }),
    );
  });

  it("never invents nodes", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        const original = new Set(g.nodes.map((x) => x.id));
        for (const node of n.nodes) expect(original.has(node.id)).toBe(true);
      }),
    );
  });

  it("caps the node count and reports when it did", () => {
    fc.assert(
      fc.property(arbGraph(), fc.integer({ min: 0, max: 6 }), (g, cap) => {
        const n = normalizeGraph(g, cap);
        expect(n.nodes.length).toBeLessThanOrEqual(cap);
        const distinct = new Set(g.nodes.map((x) => x.id)).size;
        expect(n.truncated).toBe(distinct > cap);
      }),
    );
  });
});

describe("layoutGraph", () => {
  it("preserves every node", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        expect(layoutGraph(n, 640, 480).nodes.length).toBe(n.nodes.length);
      }),
    );
  });

  it("keeps every coordinate inside the viewbox", () => {
    fc.assert(
      fc.property(
        arbGraph(),
        fc.integer({ min: 100, max: 1200 }),
        fc.integer({ min: 100, max: 1200 }),
        (g, w, h) => {
          const layout = layoutGraph(normalizeGraph(g), w, h);
          for (const node of layout.nodes) {
            expect(node.x).toBeGreaterThanOrEqual(0);
            expect(node.x).toBeLessThanOrEqual(w);
            expect(node.y).toBeGreaterThanOrEqual(0);
            expect(node.y).toBeLessThanOrEqual(h);
          }
        },
      ),
    );
  });

  it("is deterministic across calls", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        expect(layoutGraph(n, 640, 480)).toEqual(layoutGraph(n, 640, 480));
      }),
    );
  });

  it("produces one drawable line per surviving edge", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        expect(layoutGraph(n, 640, 480).edges.length).toBe(n.edges.length);
      }),
    );
  });
});

describe("neighborsOf", () => {
  it("is symmetric: a is a neighbour of b iff b is one of a", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        for (const node of n.nodes) {
          for (const other of neighborsOf(n, node.id)) {
            expect(neighborsOf(n, other).has(node.id)).toBe(true);
          }
        }
      }),
    );
  });
});
