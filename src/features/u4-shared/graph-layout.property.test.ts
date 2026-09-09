// Property-based tests for graph normalization (PBT-03).
//
// On failure fast-check prints the shrunk counterexample and its seed; replay
// it with `fc.assert(prop, { seed: <seed> })` (PBT-08).

import { describe, expect, it } from "vitest";
import fc from "fast-check";
import type { GraphDto } from "../../shared/contracts";
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

  it("prunes a topic hub the cap stranded below two members", () => {
    // The hub connects three facts (degree 3) so the cap keeps it, but a cap of
    // 2 keeps only the hub plus one fact — stranding the hub at one member. It
    // must be dropped rather than left as a lone pendant, and no edge may dangle
    // to it.
    const g: GraphDto = {
      nodes: [
        { id: "f1", label: "A", kind: "fact" },
        { id: "f2", label: "B", kind: "fact" },
        { id: "f3", label: "C", kind: "fact" },
        { id: "hub", label: "t", kind: "topic" },
      ],
      edges: [
        { from: "f1", to: "hub" },
        { from: "f2", to: "hub" },
        { from: "f3", to: "hub" },
      ],
    };
    const n = normalizeGraph(g, 2);
    expect(n.truncated).toBe(true);
    expect(n.nodes.some((x) => x.kind === "topic")).toBe(false);
    const ids = new Set(n.nodes.map((x) => x.id));
    for (const e of n.edges) {
      expect(ids.has(e.from) && ids.has(e.to)).toBe(true);
    }
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
