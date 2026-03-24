import { Badge, Button, cn, panelSurfaceClassName } from "../ui";

export function SessionVaultHeader({
  visibleCount,
  totalCount,
  timelineCount,
  scopeLabel,
  captureActivePane,
  clearAll,
  close,
}: {
  visibleCount: number;
  totalCount: number;
  timelineCount: number;
  scopeLabel: string;
  captureActivePane: () => void;
  clearAll: () => void;
  close: () => void;
}) {
  return (
    <div className="grid gap-[var(--space-4)] border-b border-[var(--border-subtle)] bg-[var(--card)] px-[var(--space-5)] py-[var(--space-5)]">
      <div className="flex flex-wrap items-start justify-between gap-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="timeline">Recall Archive</Badge>
            <Badge variant="default">{scopeLabel}</Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[1.375rem] font-semibold leading-none text-[var(--text-primary)]">
              Session Vault
            </span>
            <span className="max-w-3xl text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Capture transcripts, scrub execution history, and recover terminal state from
              checkpoints and replayable command timelines.
            </span>
          </div>
        </div>
        <div className="flex flex-wrap gap-[var(--space-2)]">
          <Button onClick={captureActivePane} variant="secondary" size="sm" title="Capture active pane now">
            Capture
          </Button>
          <Button onClick={clearAll} variant="destructive" size="sm" title="Clear all">
            Purge
          </Button>
          <Button onClick={close} variant="ghost" size="sm">
            Close
          </Button>
        </div>
      </div>
      <div className="grid gap-[var(--space-3)] md:grid-cols-2 xl:grid-cols-4">
        <MetricCard label="Visible" value={String(visibleCount)} tone="timeline" />
        <MetricCard label="Total" value={String(totalCount)} tone="default" />
        <MetricCard label="Timeline" value={String(timelineCount)} tone="accent" />
        <MetricCard label="Scope" value={scopeLabel} tone="default" />
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
  tone: "default" | "timeline" | "accent";
}) {
  const accentClasses =
    tone === "timeline"
      ? "border-[var(--timeline-border)]/80"
      : tone === "accent"
        ? "border-[var(--accent-border)]/80"
        : "";

  return (
    <div
      className={cn(
        panelSurfaceClassName,
        "grid gap-[var(--space-2)] rounded-[var(--radius-lg)] bg-[var(--panel)]/65 px-[var(--space-4)] py-[var(--space-3)] shadow-none"
      , accentClasses)}
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
