// Inline style tokens shared by the U4 views.
//
// Inline styles rather than a stylesheet: U1 owns the app shell and therefore
// the global CSS strategy, so U4 keeps its visuals self-contained and leaves
// that decision open.

export const card: React.CSSProperties = {
  border: "1px solid var(--border)",
  borderRadius: 10,
  padding: "14px 16px",
  background: "var(--card)",
};

export const muted: React.CSSProperties = { color: "var(--muted)", fontSize: 13 };

export const badge = (scope: string): React.CSSProperties => ({
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 999,
  background:
    scope === "Company" ? "#edf0e8" : scope === "Personal" ? "#f4ece0" : "#f1f1f3",
  color: scope === "Company" ? "#476044" : scope === "Personal" ? "#8b6c44" : "#5c5c62",
});

export const scopeLabel = (scope: string): string =>
  scope === "Company" ? "업무" : scope === "Personal" ? "개인" : "미분류";


/** Korean label for a fact kind. */
export const kindLabel = (kind: string): string =>
  kind === "Concern"
    ? "걸림"
    : kind === "Practice"
      ? "방식"
      : kind === "Preference"
        ? "선호"
        : kind === "Project"
          ? "진행 중"
          : "기록";

/**
 * Friction is the one kind worth spending color on — it is what the owner is
 * looking for when they scan the list. The rest stay neutral so it stands out.
 */
export const kindBadge = (kind: string): React.CSSProperties => ({
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 999,
  background: kind === "Concern" ? "#fdeceb" : "#f1f1f3",
  color: kind === "Concern" ? "#a8322a" : "#5c5c62",
});

/** Topic chip. */
export const topicChip: React.CSSProperties = {
  fontSize: 11,
  padding: "2px 7px",
  borderRadius: 4,
  background: "#eef2f7",
  color: "#41566b",
};

/** Korean label for a visibility tag. */
export const visibilityLabel = (v: string): string =>
  v === "Shared" ? "공개" : "비공개";

/**
 * Sharing is the exceptional state, so it is the one that carries color —
 * scanning the list should make "what have I exposed" answerable at a glance.
 */
export const visibilityBadge = (v: string): React.CSSProperties => ({
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 999,
  background: v === "Shared" ? "#e8f0fe" : "#f1f1f3",
  color: v === "Shared" ? "#1a56b8" : "#5c5c62",
});
