import { heartbeatColors } from "./helpers";
import type { HeartbeatItem } from "./types";

export function HeartbeatCard({ item }: { item: HeartbeatItem }) {
  const resultColor = item.last_result ? (heartbeatColors[item.last_result] || "var(--text-muted)") : "var(--text-muted)";

  return (
    <div
      style={{
        padding: "var(--space-3)",
        borderRadius: "var(--radius-md)",
        border: "1px solid var(--border)",
        background: "var(--bg-secondary)",
        marginBottom: "var(--space-2)",
        opacity: item.enabled ? 1 : 0.5,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
        <div
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: resultColor,
            flexShrink: 0,
          }}
        />
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: "var(--text-sm)", fontWeight: 500, color: "var(--text-primary)" }}>
            {item.label}
          </div>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 1 }}>
            {item.last_run_at ? `Last: ${new Date(item.last_run_at).toLocaleTimeString()}` : "Never run"}
            {item.interval_minutes > 0 && ` (every ${item.interval_minutes}m)`}
          </div>
        </div>
      </div>
      {item.last_message && item.last_result !== "ok" && (
        <div style={{ fontSize: "var(--text-xs)", color: resultColor, marginTop: "var(--space-2)", paddingLeft: 16 }}>
          {item.last_message.slice(0, 200)}
        </div>
      )}
    </div>
  );
}
