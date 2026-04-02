import type { CSSProperties } from "react";

export const sectionLabelStyle: CSSProperties = {
  fontSize: "var(--text-xs)",
  color: "var(--text-muted)",
  marginBottom: "var(--space-2)",
  fontWeight: 600,
};

export const detailLabelStyle: CSSProperties = {
  fontSize: "var(--text-xs)",
  color: "var(--text-muted)",
};

export const detailBodyStyle: CSSProperties = {
  padding: "var(--space-2)",
  borderRadius: "var(--radius-sm)",
  background: "var(--bg-tertiary)",
  fontSize: "var(--text-sm)",
  color: "var(--text-primary)",
  marginTop: 4,
  whiteSpace: "pre-wrap",
};

export const inputRowStyle: CSSProperties = {
  padding: "var(--space-2) var(--space-3)",
  borderRadius: "var(--radius-md)",
  border: "1px solid var(--border)",
  background: "var(--bg-tertiary)",
  color: "var(--text-primary)",
  fontSize: "var(--text-sm)",
  outline: "none",
};

export const inputBlockStyle: CSSProperties = {
  ...inputRowStyle,
  resize: "vertical",
  fontFamily: "inherit",
};
