// Inline style tokens shared by the U4 views.
//
// Inline styles rather than a stylesheet: U1 owns the app shell and therefore
// the global CSS strategy, so U4 keeps its visuals self-contained and leaves
// that decision open.

export const card: React.CSSProperties = {
  border: "1px solid #e3e3e6",
  borderRadius: 10,
  padding: "14px 16px",
  background: "#fff",
};

export const muted: React.CSSProperties = { color: "#6b6b70", fontSize: 13 };

export const badge = (scope: string): React.CSSProperties => ({
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 999,
  background:
    scope === "Company" ? "#e8f0fe" : scope === "Personal" ? "#eaf7ee" : "#f1f1f3",
  color: scope === "Company" ? "#1a56b8" : scope === "Personal" ? "#1c7a3d" : "#5c5c62",
});

export const scopeLabel = (scope: string): string =>
  scope === "Company" ? "업무" : scope === "Personal" ? "개인" : "미분류";
