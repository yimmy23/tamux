import type { CSSProperties } from "react";

export const actionBtnStyle: CSSProperties = {
    background: "rgba(255,255,255,0.04)",
    border: "1px solid rgba(255,255,255,0.08)",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 13,
    padding: "7px 10px",
    borderRadius: 0,
};

export function formatTime(ts: number): string {
    const date = new Date(ts);
    const now = Date.now();
    const diff = now - ts;
    if (diff < 60_000) return "just now";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
    return date.toLocaleDateString();
}
