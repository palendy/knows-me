// Round-trip properties for the wire DTOs (PBT-02).
//
// These types cross the Tauri boundary and the local HTTP API as JSON, so a
// value must survive encode -> decode unchanged. The Rust side asserts the same
// property against the same shapes in `persona/properties.rs`.

import { describe, expect, it } from "vitest";
import fc from "fast-check";
import type { DashboardDto, DraftRequest, GraphDto, MiniHomeDto } from "../../shared/contracts";
import { arbFacts, arbGraph, arbText } from "./testgen";
import { selectHighlights } from "./selection";

const roundTrip = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

describe("wire DTO round-trips", () => {
  it("GraphDto survives JSON encoding", () => {
    fc.assert(
      fc.property(arbGraph(), (g: GraphDto) => {
        expect(roundTrip(g)).toEqual(g);
      }),
    );
  });

  it("MiniHomeDto survives JSON encoding", () => {
    fc.assert(
      fc.property(arbFacts(), (facts) => {
        const dto: MiniHomeDto = { highlights: selectHighlights(facts, 9) };
        expect(roundTrip(dto)).toEqual(dto);
      }),
    );
  });

  it("DashboardDto survives JSON encoding", () => {
    fc.assert(
      fc.property(
        arbFacts(),
        fc.nat({ max: 500 }),
        fc.nat({ max: 500 }),
        (facts, collected, pending) => {
          const dto: DashboardDto = {
            collected_count: collected,
            pending_queue: pending,
            recent_facts: selectHighlights(facts, 5),
          };
          expect(roundTrip(dto)).toEqual(dto);
        },
      ),
    );
  });

  it("DraftRequest survives JSON encoding for every kind", () => {
    fc.assert(
      fc.property(
        fc.constantFrom<DraftRequest["kind"]>("Email", "Message", "Post"),
        arbText(),
        (kind, prompt) => {
          const req: DraftRequest = { kind, prompt };
          expect(roundTrip(req)).toEqual(req);
        },
      ),
    );
  });
});
