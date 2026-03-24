import { Badge, Button, cn, panelSurfaceClassName } from "../ui";

export function CommandLogHeader({
  visibleCount,
  totalCount,
  failureCount,
  runningCount,
  exportVisible,
  clearAll,
  close,
}: {
  visibleCount: number;
  totalCount: number;
  failureCount: number;
  runningCount: number;
  exportVisible: () => void;
  clearAll: () => void;
  close: () => void;
}) {
  return (
    <div className="grid gap-[var(--space-4)] border-b border-[var(--border-subtle)] bg-[var(--card)] px-[var(--space-5)] py-[var(--space-5)]">
      <div className="flex flex-wrap items-start justify-between gap-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="timeline">Execution Audit</Badge>
            <Badge variant={failureCount > 0 ? "danger" : runningCount > 0 ? "warning" : "success"}>
              {failureCount > 0 ? "Needs attention" : runningCount > 0 ? "Live activity" : "Stable"}
            </Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[1.375rem] font-semibold leading-none text-[var(--text-primary)]">
              Mission Command Log
            </span>
            <span className="max-w-3xl text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Inspect command history, replay known-good steps, and filter execution across
              workspaces, surfaces, and panes.
            </span>
          </div>
        </div>

        <div className="flex flex-wrap gap-[var(--space-2)]">
          <Button variant="secondary" size="sm" onClick={exportVisible} title="Export visible entries">
            Export
          </Button>
          <Button variant="destructive" size="sm" onClick={clearAll} title="Clear all">
            Purge
          </Button>
          <Button variant="ghost" size="sm" onClick={close} title="Close">
            Close
          </Button>
        </div>
      </div>

      <div className="grid gap-[var(--space-3)] md:grid-cols-2 xl:grid-cols-4">
        <MetricCard label="Visible" value={String(visibleCount)} tone="timeline" />
        <MetricCard label="Total" value={String(totalCount)} tone="default" />
        <MetricCard label="Failures" value={String(failureCount)} tone={failureCount > 0 ? "danger" : "success"} />
        <MetricCard label="Running" value={String(runningCount)} tone={runningCount > 0 ? "warning" : "default"} />
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
  tone: "default" | "timeline" | "success" | "warning" | "danger";
}) {
  const accentClasses =
    tone === "timeline"
      ? "border-[var(--timeline-border)]/80"
      : tone === "success"
        ? "border-[var(--success-border)]/80"
        : tone === "warning"
          ? "border-[var(--warning-border)]/80"
          : tone === "danger"
            ? "border-[var(--danger-border)]/80"
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
      <span className="text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">{value}</span>
    </div>
  );
}
