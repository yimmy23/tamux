import type { CognitiveEvent } from "../lib/agentMissionStore";

export function ReasoningStream({ events }: { events: CognitiveEvent[] }) {
  if (events.length === 0) {
    return (
      <div className="zorai-empty-state">
        <div className="zorai-empty-state__icon">◉</div>
        <div className="zorai-empty-state__title">No reasoning trace</div>
        <div className="zorai-empty-state__description">Cognitive events will appear here as the agent processes</div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      {events.map((event) => (
        <div
          key={event.id}
          style={{
            padding: "var(--space-4)",
            borderRadius: "var(--radius-lg)",
            border: "1px solid var(--glass-border)",
            background: "var(--bg-secondary)",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              gap: "var(--space-3)",
              marginBottom: "var(--space-2)",
            }}
          >
            <span
              style={{
                fontSize: "var(--text-xs)",
                color: "var(--reasoning)",
                letterSpacing: "0.1em",
                textTransform: "uppercase",
                fontWeight: 600,
              }}
            >
              {event.source}
            </span>
            
            <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
              {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
            </span>
          </div>
          
          <div
            style={{
              whiteSpace: "pre-wrap",
              fontSize: "var(--text-sm)",
              lineHeight: 1.7,
              fontStyle: "italic",
              color: "var(--text-secondary)",
            }}
          >
            {event.content}
          </div>
        </div>
      ))}
    </div>
  );
}
