import { useAuditStore, filteredEntries } from "../../lib/auditStore";
import { AuditRow } from "./AuditRow";

/**
 * Scrollable chronological list of audit entries.
 * Newest entries at top (entries already sorted by timestamp desc in store).
 */
export function AuditList() {
  const entries = useAuditStore((s) => filteredEntries(s));
  const selectedEntryId = useAuditStore((s) => s.selectedEntryId);
  const selectEntry = useAuditStore((s) => s.selectEntry);

  if (entries.length === 0) {
    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: "var(--space-8)",
          textAlign: "center",
        }}
      >
        <div
          style={{
            fontSize: "var(--text-base)",
            fontWeight: "var(--font-bold)",
            color: "var(--text-primary)",
            marginBottom: "var(--space-2)",
          }}
        >
          No actions recorded
        </div>
        <div
          style={{
            fontSize: "var(--text-base)",
            color: "var(--text-secondary)",
            lineHeight: "var(--leading-normal)",
            maxWidth: 300,
          }}
        >
          The agent hasn't taken any autonomous actions yet. Actions will appear here as the
          heartbeat runs and the agent works on tasks.
        </div>
      </div>
    );
  }

  return (
    <div style={{ flex: 1, overflowY: "auto" }}>
      {entries.map((entry) => (
        <AuditRow
          key={entry.id}
          entry={entry}
          isSelected={selectedEntryId === entry.id}
          onSelect={() => selectEntry(entry.id)}
        />
      ))}
    </div>
  );
}
