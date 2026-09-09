// Reusable fast-check domain generators (PBT-07).
//
// Every property test draws from these rather than raw primitives, so the
// generated values obey the same constraints real data does: valid uuid-shaped
// ids, non-empty titles, `confirmed_at` present exactly when confirmed, and
// graph edges that mostly point at real nodes (with dangling ones deliberately
// mixed in, since removing those is what normalization is for).

import fc from "fast-check";
import type { Fact, GraphDto, GraphEdge, GraphNode, GraphNodeKind, Scope } from "../../shared/contracts";

export const arbUuid = (): fc.Arbitrary<string> =>
  fc
    .integer({ min: 0, max: 999_999 })
    .map((n) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`);

export const arbScope = (): fc.Arbitrary<Scope> =>
  fc.constantFrom<Scope>("Company", "Personal", "Unknown");

/** Realistic titles/bodies: Hangul, latin, and the awkward boundary cases. */
export const arbText = (): fc.Arbitrary<string> =>
  fc.oneof(
    { weight: 3, arbitrary: fc.string({ minLength: 1, maxLength: 30 }) },
    { weight: 2, arbitrary: fc.constantFrom("배포 절차", "코드 리뷰", "테스트 명령") },
    { weight: 1, arbitrary: fc.constant("a") },
    { weight: 1, arbitrary: fc.constant("  공백  많은  제목  ") },
  );

const arbIsoDate = (): fc.Arbitrary<string> =>
  fc
    .integer({ min: 1, max: 28 })
    .map((d) => `2026-09-${String(d).padStart(2, "0")}T09:00:00Z`);

export const arbFact = (): fc.Arbitrary<Fact> =>
  fc
    .record({
      id: arbUuid(),
      title: arbText(),
      body: arbText(),
      links: fc.array(arbUuid(), { maxLength: 5 }),
      confirmed: fc.boolean(),
      scope: arbScope(),
      confirmedAt: arbIsoDate(),
    })
    .map(({ confirmed, scope, confirmedAt, ...rest }) => ({
      ...rest,
      metadata: {
        provenance: {
          source: "Session" as const,
          collected_at: "2026-09-01T09:00:00Z",
        },
        confirmed,
        scope,
        confirmed_at: confirmed ? confirmedAt : null,
      },
    }));

/**
 * A fact *set*: ids are unique, because `FactId` is a primary key. Generating
 * two different facts under one id would be modelling something that cannot
 * exist, and any property about ordering would be meaningless for it (PBT-07).
 */
export const arbFacts = (): fc.Arbitrary<Fact[]> =>
  fc.uniqueArray(arbFact(), { maxLength: 20, selector: (f) => f.id });

/**
 * A graph whose edges mostly connect real nodes, plus 0-3 deliberately
 * dangling edges so `normalizeGraph` has something to remove.
 */
export const arbGraph = (): fc.Arbitrary<GraphDto> =>
  fc
    .array(
      fc.record({
        id: arbUuid(),
        label: arbText(),
        kind: fc.constantFrom<GraphNodeKind>("fact", "topic"),
      }),
      { maxLength: 12 },
    )
    .chain((rawNodes) => {
      // Ids may repeat by construction; dedupe so "nodes" is a real node set.
      const nodes: GraphNode[] = [];
      const seen = new Set<string>();
      for (const n of rawNodes) {
        if (seen.has(n.id)) continue;
        seen.add(n.id);
        nodes.push(n);
      }
      const ids = nodes.map((n) => n.id);

      const realEdges: fc.Arbitrary<GraphEdge[]> =
        ids.length === 0
          ? fc.constant([])
          : fc.array(
              fc.record({
                from: fc.constantFrom(...ids),
                to: fc.constantFrom(...ids),
              }),
              { maxLength: 20 },
            );

      return fc
        .record({
          edges: realEdges,
          dangling: fc.array(arbUuid(), { maxLength: 3 }),
        })
        .map(({ edges, dangling }) => ({
          nodes,
          edges: [
            ...edges,
            ...dangling.map((to) => ({ from: ids[0] ?? "ghost", to })),
          ],
        }));
    });

/** Deterministic reorder, for "input order must not matter" properties. */
export function rotate<T>(items: readonly T[], by: number): T[] {
  if (items.length === 0) return [];
  const k = ((by % items.length) + items.length) % items.length;
  return [...items.slice(k), ...items.slice(0, k)];
}
