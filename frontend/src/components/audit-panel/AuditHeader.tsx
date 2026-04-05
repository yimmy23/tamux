import { useAuditStore } from "../../lib/auditStore";
import type { ActionType, TimeRange } from "../../lib/types";

const ALL_ACTION_TYPES: ActionType[] = ["heartbeat", "tool", "escalation", "skill", "subagent"];

const TIME_RANGE_LABELS: Record<TimeRange, string> = {
  last_hour: "Last hour",
  today: "Today",
  this_week: "This week",
  all_time: "All time",
};

const TYPE_COLORS: Record<ActionType, string> = {
  heartbeat: "var(--agent)",
  tool: "var(--agent)",
  escalation: "var(--approval)",
  skill: "var(--reasoning)",
  subagent: "var(--reasoning)",
};

/**
 * AuditHeader: title, metric cards, filter controls, and close button.
 * Per UI-SPEC AuditHeader component.
 */
export function AuditHeader({ onClose }: { onClose: () => void }) {
  const entries = useAuditStore((s) => s.entries);
  const filters = useAuditStore((s) => s.filters);
  const setTypeFilter = useAuditStore((s) => s.setTypeFilter);
  const setTimeRange = useAuditStore((s) => s.setTimeRange);
  const loadAuditHistory = useAuditStore((s) => s.loadAuditHistory);
  const loadProvenanceReport = useAuditStore((s) => s.loadProvenanceReport);
  const loadMemoryProvenanceReport = useAuditStore((s) => s.loadMemoryProvenanceReport);
  const loadingHistory = useAuditStore((s) => s.loadingHistory);
  const loadingProvenance = useAuditStore((s) => s.loadingProvenance);
  const loadingMemoryProvenance = useAuditStore((s) => s.loadingMemoryProvenance);
  const provenanceReport = useAuditStore((s) => s.provenanceReport);
  const memoryProvenanceReport = useAuditStore((s) => s.memoryProvenanceReport);
  const provenanceStatus = useAuditStore((s) => s.provenanceStatus);
  const memoryProvenanceStatus = useAuditStore((s) => s.memoryProvenanceStatus);

  const totalCount = entries.length;

  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayCount = entries.filter((e) => e.timestamp >= todayStart.getTime()).length;

  const toggleType = (type: ActionType) => {
    const next = new Set(filters.types);
    if (next.has(type)) {
      next.delete(type);
    } else {
      next.add(type);
    }
    setTypeFilter(next);
  };

  return (
    <div
      style={{
        padding: "var(--space-6)",
        borderBottom: "1px solid var(--border)",
      }}
    >
      {/* Title row + close */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: "var(--space-4)",
        }}
      >
        <span
          style={{
            fontSize: 15,
            fontWeight: "var(--font-bold)",
            color: "var(--text-primary)",
            lineHeight: "var(--leading-tight)",
          }}
        >
          Audit Feed
        </span>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button type="button" onClick={() => void loadAuditHistory()} style={headerActionStyle}>
            {loadingHistory ? "Loading..." : "Load History"}
          </button>
          <button type="button" onClick={() => void loadProvenanceReport()} style={headerActionStyle}>
            {loadingProvenance ? "Checking..." : "Verify Provenance"}
          </button>
          <button type="button" onClick={() => void loadMemoryProvenanceReport()} style={headerActionStyle}>
            {loadingMemoryProvenance ? "Loading..." : "Memory Provenance"}
          </button>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close audit panel"
            style={{
              background: "none",
              border: "none",
              color: "var(--text-secondary)",
              fontSize: 18,
              cursor: "pointer",
              padding: "2px 4px",
              lineHeight: 1,
            }}
          >
            &times;
          </button>
        </div>
      </div>

      {provenanceReport ? (
        <div
          style={{
            marginBottom: "var(--space-3)",
            padding: "var(--space-2) var(--space-3)",
            background: "var(--bg-tertiary)",
            borderRadius: 4,
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            display: "flex",
            flexWrap: "wrap",
            gap: 10,
          }}
        >
          <span>Total {provenanceReport.totalEntries}</span>
          <span>Hash {provenanceReport.validHashEntries}/{provenanceReport.totalEntries}</span>
          <span>Signatures {provenanceReport.validSignatureEntries}/{provenanceReport.totalEntries}</span>
          <span>Chain {provenanceReport.validChainEntries}/{provenanceReport.totalEntries}</span>
        </div>
      ) : null}

      {provenanceStatus ? (
        <div style={{ marginBottom: "var(--space-3)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
          {provenanceStatus}
        </div>
      ) : null}

      {memoryProvenanceReport ? (
        <div
          style={{
            marginBottom: "var(--space-3)",
            padding: "var(--space-2) var(--space-3)",
            background: "var(--bg-tertiary)",
            borderRadius: 4,
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            display: "flex",
            flexWrap: "wrap",
            gap: 10,
          }}
        >
          <span>Memory {memoryProvenanceReport.totalEntries}</span>
          <span>Active {memoryProvenanceReport.summaryByStatus.active ?? 0}</span>
          <span>Uncertain {memoryProvenanceReport.summaryByStatus.uncertain ?? 0}</span>
          <span>Retracted {memoryProvenanceReport.summaryByStatus.retracted ?? 0}</span>
        </div>
      ) : null}

      {memoryProvenanceStatus ? (
        <div style={{ marginBottom: "var(--space-3)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
          {memoryProvenanceStatus}
        </div>
      ) : null}

      {/* Metric cards */}
      <div
        style={{
          display: "flex",
          gap: "var(--space-2)",
          marginBottom: "var(--space-4)",
        }}
      >
        <MetricCard label="Total" value={totalCount} />
        <MetricCard label="Today" value={todayCount} />
      </div>

      {/* Type toggles */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "var(--space-1)",
          marginBottom: "var(--space-2)",
        }}
      >
        {ALL_ACTION_TYPES.map((type) => {
          const active = filters.types.has(type);
          return (
            <button
              key={type}
              type="button"
              onClick={() => toggleType(type)}
              style={{
                fontSize: "var(--text-xs)",
                fontWeight: "var(--font-bold)",
                padding: "2px 8px",
                borderRadius: 4,
                border: "1px solid var(--border)",
                background: active ? "var(--bg-tertiary)" : "transparent",
                color: active ? TYPE_COLORS[type] : "var(--text-secondary)",
                cursor: "pointer",
                textTransform: "uppercase",
                opacity: active ? 1 : 0.5,
              }}
            >
              {type}
            </button>
          );
        })}
      </div>

      {/* Time range selector */}
      <select
        value={filters.timeRange}
        onChange={(e) => setTimeRange(e.target.value as TimeRange)}
        style={{
          fontSize: "var(--text-xs)",
          padding: "2px 6px",
          background: "var(--bg-tertiary)",
          color: "var(--text-primary)",
          border: "1px solid var(--border)",
          borderRadius: 4,
          cursor: "pointer",
        }}
      >
        {(Object.keys(TIME_RANGE_LABELS) as TimeRange[]).map((range) => (
          <option key={range} value={range}>
            {TIME_RANGE_LABELS[range]}
          </option>
        ))}
      </select>
    </div>
  );
}

const headerActionStyle = {
  fontSize: "var(--text-xs)",
  padding: "2px 8px",
  background: "var(--bg-tertiary)",
  color: "var(--text-primary)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  cursor: "pointer",
};

function MetricCard({ label, value }: { label: string; value: number }) {
  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        padding: "var(--space-2) var(--space-4)",
        borderRadius: 4,
        minWidth: 60,
        textAlign: "center",
      }}
    >
      <div
        style={{
          fontSize: "var(--text-base)",
          fontWeight: "var(--font-bold)",
          color: "var(--text-primary)",
        }}
      >
        {value}
      </div>
      <div
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--text-secondary)",
        }}
      >
        {label}
      </div>
    </div>
  );
}
