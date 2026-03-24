import type { AuditEntry } from "../../lib/types";
import { ConfidenceBadge } from "./ConfidenceBadge";

/**
 * Expanded detail view for a single audit entry.
 * Shows full explanation, confidence breakdown, causal trace ID, and thread ID.
 */
export function AuditDetailView({ entry }: { entry: AuditEntry }) {
  return (
    <div
      style={{
        padding: "var(--space-4)",
        background: "var(--reasoning-soft)",
        borderTop: "1px solid var(--border)",
      }}
    >
      {entry.explanation && (
        <div
          style={{
            fontSize: "var(--text-base)",
            color: "var(--text-secondary)",
            marginBottom: "var(--space-2)",
            lineHeight: "var(--leading-normal)",
          }}
        >
          {entry.explanation}
        </div>
      )}

      {entry.confidence != null && entry.confidenceBand && (
        <div style={{ marginBottom: "var(--space-2)" }}>
          <ConfidenceBadge confidence={entry.confidence} band={entry.confidenceBand} />
        </div>
      )}

      {entry.causalTraceId && (
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            marginBottom: "var(--space-1)",
          }}
        >
          Causal trace: <span style={{ color: "var(--reasoning)" }}>{entry.causalTraceId}</span>
        </div>
      )}

      {entry.threadId && (
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
          }}
        >
          Thread: <span style={{ color: "var(--agent)" }}>{entry.threadId}</span>
        </div>
      )}
    </div>
  );
}
