import type { CSSProperties } from "react";

export const contextMenuItemStyle: CSSProperties = {
  border: "none",
  background: "transparent",
  color: "var(--text-primary)",
  padding: "6px 8px",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  textAlign: "left",
  fontSize: "var(--text-sm)",
};

export const dangerContextMenuItemStyle: CSSProperties = {
  ...contextMenuItemStyle,
  color: "var(--danger)",
};

export const contextMenuSectionLabelStyle: CSSProperties = {
  padding: "6px 8px 2px",
  color: "var(--text-muted)",
  fontSize: "var(--text-2xs)",
  textTransform: "uppercase",
  letterSpacing: "0.04em",
};
