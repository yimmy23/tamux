import { SectionTitle } from "../shared";
import { HeartbeatCard } from "./HeartbeatCard";
import type { HeartbeatItem } from "./types";

export function HeartbeatSection({
  heartbeatItems,
}: {
  heartbeatItems: HeartbeatItem[];
}) {
  return (
    <>
      <SectionTitle
        title="Heartbeat"
        subtitle="Periodic health checks run by the daemon"
      />

      {heartbeatItems.length > 0 ? (
        heartbeatItems.map((item) => <HeartbeatCard key={item.id} item={item} />)
      ) : (
        <div
          style={{
            textAlign: "center",
            padding: "var(--space-4)",
            color: "var(--text-muted)",
            fontSize: "var(--text-xs)",
          }}
        >
          No heartbeat checks configured. Edit ~/.zorai/agent/heartbeat.json to
          add checks.
        </div>
      )}
    </>
  );
}
