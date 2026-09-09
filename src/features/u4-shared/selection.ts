// Mini-home highlight ranking (US-5.2, BR-V2).
//
// This mirrors `src-tauri/src/persona/selection.rs`. It exists on the frontend
// so the view can rank a raw fact list when one is available locally; both
// implementations follow the same total order, and both are covered by the
// same invariants.

import type { Fact, FactSummary } from "../../shared/contracts";

export const DEFAULT_HIGHLIGHTS = 9;

/**
 * Rank confirmed facts by how representative they are.
 *
 * Order: link degree desc, title asc, id asc — matching the Rust side exactly.
 * The recency tiebreaker the rule originally carried is gone because the
 * backend ranks from `FactSummary`, which has no timestamp; keeping it here
 * would make the two implementations disagree. The final id tiebreaker is what
 * makes this a *total* order — without it the result would still depend on
 * input order (BR-V4).
 */
export function selectHighlights(
  facts: readonly Fact[],
  limit: number = DEFAULT_HIGHLIGHTS,
): FactSummary[] {
  const confirmed = facts.filter((f) => f.metadata.confirmed);

  confirmed.sort((a, b) => {
    const byDegree = b.links.length - a.links.length;
    if (byDegree !== 0) return byDegree;

    const byTitle = a.title.localeCompare(b.title);
    if (byTitle !== 0) return byTitle;

    return a.id.localeCompare(b.id);
  });

  // Dedupe *after* sorting: if an id ever shows up twice, the survivor is
  // chosen by the same total order as everything else, not by arrival order.
  const seen = new Set<string>();
  const unique = confirmed.filter((f) => {
    if (seen.has(f.id)) return false;
    seen.add(f.id);
    return true;
  });

  return unique.slice(0, Math.max(0, limit)).map((f) => ({
    id: f.id,
    title: f.title,
    scope: f.metadata.scope,
    kind: f.metadata.kind ?? "Note",
    topics: f.metadata.topics ?? [],
    visibility: f.metadata.visibility ?? "Private",
    confirmed: f.metadata.confirmed,
  }));
}
