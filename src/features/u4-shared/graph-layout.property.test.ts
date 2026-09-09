// Property-based tests for graph normalization (PBT-03).
//
// On failure fast-check prints the shrunk counterexample and its seed; replay
// it with `fc.assert(prop, { seed: <seed> })` (PBT-08).

import { describe, expect, it } from "vitest";
import fc from "fast-check";
import { normalizeGraph } from "./graph-layout";
import { neighborsOf, toForceGraph } from "./graph-force";
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

describe("toForceGraph", () => {
  it("emits one link per surviving edge and one node per surviving node", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const n = normalizeGraph(g);
        const f = toForceGraph(g);
        expect(f.nodes.length).toBe(n.nodes.length);
        expect(f.links.length).toBe(n.edges.length);
      }),
    );
  });

  it("gives every link endpoints that exist as nodes", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const f = toForceGraph(g);
        const present = new Set(f.nodes.map((x) => x.id));
        for (const l of f.links) {
          expect(present.has(l.source)).toBe(true);
          expect(present.has(l.target)).toBe(true);
        }
      }),
    );
  });
});

describe("neighborsOf", () => {
  it("is symmetric: a is a neighbour of b iff b is one of a", () => {
    fc.assert(
      fc.property(arbGraph(), (g) => {
        const f = toForceGraph(g);
        for (const node of f.nodes) {
          for (const other of neighborsOf(f, node.id)) {
            expect(neighborsOf(f, other).has(node.id)).toBe(true);
          }
        }
      }),
    );
  });
});
