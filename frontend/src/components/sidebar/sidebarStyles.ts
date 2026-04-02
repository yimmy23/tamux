import type { CSSProperties } from "react";

export const treeToggleStyle: CSSProperties = {
  border: "none",
  background: "transparent",
  color: "var(--text-muted)",
  cursor: "pointer",
  fontSize: "var(--text-xs)",
  width: 14,
  height: 14,
  padding: 0,
  lineHeight: 1,
};

export function treeNodeButtonStyle(
  active: boolean,
  accent: string,
): CSSProperties {
  return {
    border: "none",
    background: "transparent",
    color: active ? "var(--text-primary)" : "var(--text-secondary)",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: 0,
    minWidth: 0,
    flex: 1,
    textAlign: "left",
    fontSize: "var(--text-xs)",
    fontWeight: active ? 600 : 500,
    borderLeft: active ? `2px solid ${accent}` : "2px solid transparent",
    paddingLeft: active ? 6 : 8,
  };
}

export function surfaceNodeStyle(active: boolean): CSSProperties {
  return {
    border: "none",
    background: "transparent",
    color: active ? "var(--text-primary)" : "var(--text-muted)",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: 0,
    minWidth: 0,
    flex: 1,
    textAlign: "left",
    fontSize: "var(--text-xs)",
    fontWeight: active ? 600 : 500,
  };
}

export function paneNodeButtonStyle(needsApproval: boolean): CSSProperties {
  return {
    border: "none",
    background: "transparent",
    color: needsApproval ? "var(--approval)" : "var(--text-secondary)",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: 0,
    minWidth: 0,
    flex: 1,
    textAlign: "left",
    fontSize: "var(--text-xs)",
  };
}

export function paneCountBadgeStyle(needsApproval: boolean): CSSProperties {
  return {
    marginLeft: "auto",
    background: needsApproval ? "var(--approval-soft)" : "var(--bg-tertiary)",
    border: "1px solid",
    borderColor: needsApproval ? "var(--approval-border)" : "var(--glass-border)",
    color: needsApproval ? "var(--approval)" : "var(--text-muted)",
    borderRadius: "var(--radius-full)",
    fontSize: 10,
    fontWeight: 700,
    lineHeight: "16px",
    minWidth: 16,
    textAlign: "center",
    padding: "0 4px",
  };
}

export const paneRenameInputStyle: CSSProperties = {
  width: "100%",
  background: "var(--bg-secondary)",
  border: "1px solid var(--glass-border)",
  borderRadius: "var(--radius-sm)",
  color: "var(--text-primary)",
  fontSize: "var(--text-xs)",
  padding: "2px 4px",
};

export const pendingDotStyle: CSSProperties = {
  width: 6,
  height: 6,
  borderRadius: "50%",
  background: "var(--approval)",
  flexShrink: 0,
  animation: "agent-pulse 1.4s ease-in-out infinite",
};

export const countBadgeStyle: CSSProperties = {
  background: "var(--accent)",
  color: "var(--bg-primary)",
  borderRadius: "var(--radius-full)",
  padding: "0 6px",
  fontSize: "var(--text-xs)",
  fontWeight: 700,
  minWidth: 18,
  textAlign: "center",
  lineHeight: "16px",
};

export const contextMenuItemStyle: CSSProperties = {
  border: "none",
  background: "transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
  textAlign: "left",
  fontSize: "var(--text-xs)",
  padding: "6px 8px",
  borderRadius: "var(--radius-sm)",
};

export const dangerContextMenuItemStyle: CSSProperties = {
  ...contextMenuItemStyle,
  color: "var(--danger)",
};
