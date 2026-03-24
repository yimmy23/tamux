import { Badge, Button, cn, panelSurfaceClassName } from "../ui";
import { formatUptime } from "./shared";

export function SystemMonitorHeader({
  hostname,
  platform,
  intervalMs,
  uptimeSeconds,
  clear,
  close,
}: {
  hostname: string;
  platform: string;
  intervalMs: number;
  uptimeSeconds: number | null;
  clear: () => void;
  close: () => void;
}) {
  return (
    <div className="grid gap-[var(--space-4)] border-b border-[var(--border-subtle)] bg-[var(--card)] px-[var(--space-5)] py-[var(--space-5)]">
      <div className="flex flex-wrap items-start justify-between gap-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="agent">Host Telemetry</Badge>
            <Badge variant="default">{platform}</Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[1.375rem] font-semibold leading-none text-[var(--text-primary)]">
              System Monitor
            </span>
            <span className="max-w-3xl text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Live CPU, memory, GPU, and process telemetry with adjustable refresh cadence and
              per-process inspection.
            </span>
          </div>
        </div>
        <div className="flex flex-wrap gap-[var(--space-2)]">
          <Button type="button" variant="secondary" size="sm" onClick={clear}>
            Clear
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={close}>
            Close
          </Button>
        </div>
      </div>

      <div className="grid gap-[var(--space-3)] md:grid-cols-2 xl:grid-cols-4">
        <MetricCard label="Host" value={hostname} tone="agent" />
        <MetricCard label="Platform" value={platform} tone="default" />
        <MetricCard label="Refresh" value={`every ${intervalMs / 1000}s`} tone="accent" />
        <MetricCard
          label="Uptime"
          value={uptimeSeconds === null ? "pending" : formatUptime(uptimeSeconds)}
          tone="default"
        />
      </div>
    </div>
  );
}

function MetricCard({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "default" | "agent" | "accent";
}) {
  const accentClasses =
    tone === "agent"
      ? "border-[var(--agent-border)]/80"
      : tone === "accent"
        ? "border-[var(--accent-border)]/80"
        : "";

  return (
    <div
      className={cn(
        panelSurfaceClassName,
        "grid gap-[var(--space-2)] rounded-[var(--radius-lg)] bg-[var(--panel)]/65 px-[var(--space-4)] py-[var(--space-3)] shadow-none",
        accentClasses
      )}
    >
      <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
        {label}
      </span>
      <span className="break-words text-[var(--text-base)] font-semibold text-[var(--text-primary)]">
        {value}
      </span>
    </div>
  );
}
