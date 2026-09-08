// Property-based tests for highlight ranking (PBT-03, BR-V2).

import { describe, expect, it } from "vitest";
import fc from "fast-check";
import { selectHighlights } from "./selection";
import { arbFacts, rotate } from "./testgen";

describe("selectHighlights", () => {
  it("returns a bounded subset of the confirmed facts", () => {
    fc.assert(
      fc.property(arbFacts(), fc.integer({ min: 0, max: 12 }), (facts, limit) => {
        const out = selectHighlights(facts, limit);
        expect(out.length).toBeLessThanOrEqual(limit);

        const confirmed = new Set(
          facts.filter((f) => f.metadata.confirmed).map((f) => f.id),
        );
        for (const s of out) {
          expect(confirmed.has(s.id)).toBe(true);
          expect(s.confirmed).toBe(true);
        }
      }),
    );
  });

  it("does not depend on the order facts arrive in", () => {
    fc.assert(
      fc.property(arbFacts(), fc.integer({ min: 0, max: 30 }), (facts, by) => {
        expect(selectHighlights(facts, 9).map((s) => s.id)).toEqual(
          selectHighlights(rotate(facts, by), 9).map((s) => s.id),
        );
      }),
    );
  });

  it("orders by descending link degree", () => {
    fc.assert(
      fc.property(arbFacts(), (facts) => {
        const byId = new Map(facts.map((f) => [f.id, f]));
        const degrees = selectHighlights(facts, 20).map(
          (s) => byId.get(s.id)?.links.length ?? 0,
        );
        for (let i = 1; i < degrees.length; i += 1) {
          expect(degrees[i - 1]!).toBeGreaterThanOrEqual(degrees[i]!);
        }
      }),
    );
  });

  it("returns nothing when nothing is confirmed", () => {
    fc.assert(
      fc.property(arbFacts(), (facts) => {
        const unconfirmed = facts.map((f) => ({
          ...f,
          metadata: { ...f.metadata, confirmed: false, confirmed_at: null },
        }));
        expect(selectHighlights(unconfirmed, 9)).toEqual([]);
      }),
    );
  });
});
