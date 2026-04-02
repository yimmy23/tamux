/**
 * Inline badge: verbal band + percentage + colored dot.
 * Per D-09 and UI-SPEC confidence visual indicators.
 */
export function ConfidenceBadge({ confidence, band }: { confidence: number | null; band: string | null }) {
  if (band == null || confidence == null) return null;

  const colors: Record<string, string> = {
    confident: "var(--success)",
    likely: "var(--info)",
    uncertain: "var(--warning)",
    guessing: "var(--danger)",
  };
  const dotColor = colors[band] ?? "var(--text-secondary)";
  const pct = Math.round(confidence * 100);

  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-1)" }}>
      <span
        style={{
          width: 6,
          height: 6,
          borderRadius: "50%",
          backgroundColor: dotColor,
          display: "inline-block",
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: "var(--text-xs)",
          fontWeight: "var(--font-bold)",
          color: "var(--text-secondary)",
        }}
      >
        {band} {pct}%
      </span>
    </span>
  );
}
