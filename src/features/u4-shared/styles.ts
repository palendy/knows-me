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

