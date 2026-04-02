import type { EscalationInfo } from "../../lib/types";

const LEVEL_LABELS: Record<string, string> = {
  L0: "self-correcting",
  L1: "delegating to sub-agent",
  L2: "needs your input",
  L3: "requesting external help",
};

const LEVEL_COLORS: Record<string, string> = {
  L0: "var(--agent)",
  L1: "var(--reasoning)",
  L2: "var(--approval)",
  L3: "var(--danger)",
};

/**
 * Status bar banner showing current escalation level with "I'll handle this" action.
 * Per D-12, D-13, and UI-SPEC escalation section.
 */
export function EscalationBanner({
  escalation,
  onCancel,
}: {
  escalation: EscalationInfo;
  onCancel: () => void;
}) {
  const label = LEVEL_LABELS[escalation.toLevel] ?? escalation.toLevel;
  const color = LEVEL_COLORS[escalation.toLevel] ?? "var(--text-secondary)";

  return (
    <div
      style={{
        padding: "var(--space-4)",
        background: "var(--bg-secondary)",
        borderBottom: "1px solid var(--border)",
        display: "flex",
        alignItems: "center",
        gap: "var(--space-2)",
      }}
    >
      <span
        style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          backgroundColor: color,
          flexShrink: 0,
        }}
      />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: "var(--text-base)",
            fontWeight: "var(--font-bold)",
            color: "var(--text-primary)",
          }}
        >
          Escalation: {escalation.fromLevel} &rarr; {escalation.toLevel} &mdash; {label}
        </div>
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            marginTop: "var(--space-1)",
          }}
        >
          {escalation.reason}
          {escalation.attempts > 0 && ` (${escalation.attempts} attempt${escalation.attempts !== 1 ? "s" : ""})`}
        </div>
      </div>
      <button
        type="button"
        onClick={onCancel}
        title="Take over: You'll handle this manually. The agent will stop its current escalation and wait for your instructions."
        style={{
          padding: "4px 10px",
          fontSize: "var(--text-xs)",
          fontWeight: "var(--font-bold)",
          color: "var(--danger)",
          background: "transparent",
          border: "1px solid var(--danger)",
          borderRadius: 4,
          cursor: "pointer",
          whiteSpace: "nowrap",
          flexShrink: 0,
        }}
      >
        I'll handle this
      </button>
    </div>
  );
}
