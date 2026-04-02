import { useState } from "react";
import type { AuditEntry } from "../../lib/types";
import { useAuditStore } from "../../lib/auditStore";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { AuditDetailView } from "./AuditDetailView";

const ACTION_TYPE_COLORS: Record<string, string> = {
  heartbeat: "var(--agent)",
  tool: "var(--agent)",
  escalation: "var(--approval)",
  skill: "var(--reasoning)",
  subagent: "var(--reasoning)",
};

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

/**
 * Single audit entry row with summary, explanation, confidence, and expand/collapse.
 * Per UI-SPEC Interaction Contracts (Audit Panel section).
 */
export function AuditRow({
  entry,
  isSelected,
  onSelect,
}: {
  entry: AuditEntry;
  isSelected: boolean;
  onSelect: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const dismissEntry = useAuditStore((s) => s.dismissEntry);
  const isDismissed = entry.userAction === "dismissed";
  const badgeColor = ACTION_TYPE_COLORS[entry.actionType] ?? "var(--text-secondary)";

  // Per D-10: only show confidence badge when below threshold (not "confident")
  const showConfidence =
    entry.confidence != null &&
    entry.confidenceBand != null &&
    entry.confidenceBand !== "confident";

  return (
    <div
      style={{
        background: isSelected ? "var(--bg-tertiary)" : "transparent",
        borderBottom: "1px solid var(--border)",
        opacity: isDismissed ? 0.5 : 1,
      }}
    >
      <div
        role="button"
        tabIndex={0}
        onClick={() => {
          onSelect();
          setExpanded((prev) => !prev);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onSelect();
            setExpanded((prev) => !prev);
          }
        }}
        style={{
          padding: "var(--space-4)",
          cursor: "pointer",
          display: "flex",
          alignItems: "flex-start",
          gap: "var(--space-2)",
        }}
      >
        {/* Timestamp */}
        <span
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            fontWeight: "var(--font-regular)",
            whiteSpace: "nowrap",
            flexShrink: 0,
            lineHeight: "var(--leading-snug)",
            paddingTop: 1,
          }}
        >
          {formatTimestamp(entry.timestamp)}
        </span>

        {/* Type badge */}
        <span
          style={{
            fontSize: "var(--text-xs)",
            fontWeight: "var(--font-bold)",
            color: badgeColor,
            textTransform: "uppercase",
            whiteSpace: "nowrap",
            flexShrink: 0,
            lineHeight: "var(--leading-snug)",
            paddingTop: 1,
          }}
        >
          {entry.actionType}
        </span>

        {/* Summary + explanation + confidence */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: "var(--text-base)",
              color: isDismissed ? "var(--text-secondary)" : "var(--text-primary)",
              lineHeight: "var(--leading-normal)",
              textDecoration: isDismissed ? "line-through" : "none",
            }}
          >
            {entry.summary}
          </div>

          {entry.explanation && !expanded && (
            <div
              style={{
                fontSize: "var(--text-base)",
                color: "var(--text-secondary)",
                marginTop: "var(--space-1)",
                lineHeight: "var(--leading-normal)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {entry.explanation}
            </div>
          )}

          {showConfidence && (
            <div style={{ marginTop: "var(--space-1)" }}>
              <ConfidenceBadge confidence={entry.confidence} band={entry.confidenceBand} />
            </div>
          )}
        </div>

        {/* Dismiss button */}
        {!isDismissed && (
          <button
            aria-label="Dismiss audit entry"
            title="Dismiss"
            onClick={(e) => {
              e.stopPropagation();
              dismissEntry(entry.id);
            }}
            style={{
              background: "transparent",
              border: "none",
              cursor: "pointer",
              color: "var(--text-secondary)",
              fontSize: "var(--text-xs)",
              padding: "2px 4px",
              borderRadius: "2px",
              flexShrink: 0,
              lineHeight: 1,
              opacity: 0.6,
            }}
            onMouseEnter={(e) => { e.currentTarget.style.opacity = "1"; }}
            onMouseLeave={(e) => { e.currentTarget.style.opacity = "0.6"; }}
          >
            &#10005;
          </button>
        )}

        {/* Expand/collapse chevron */}
        <span
          aria-label={expanded ? "Collapse audit details" : "Expand audit details"}
          style={{
            fontSize: "var(--text-base)",
            color: "var(--accent)",
            flexShrink: 0,
            transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
            transition: "transform 0.15s ease",
            lineHeight: 1,
          }}
        >
          &#9654;
        </span>
      </div>

      {expanded && <AuditDetailView entry={entry} />}
    </div>
  );
}
