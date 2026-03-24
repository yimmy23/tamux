import { Badge, Button, cn, panelSurfaceClassName } from "../ui";

export function NotificationHeader({
  unreadCount,
  totalCount,
  markAllRead,
  clearAll,
  close,
}: {
  unreadCount: number;
  totalCount: number;
  markAllRead: () => void;
  clearAll: () => void;
  close: () => void;
}) {
  return (
    <div className="grid gap-[var(--space-4)] border-b border-[var(--border-subtle)] bg-[var(--card)] px-[var(--space-4)] py-[var(--space-5)]">
      <div className="flex items-start justify-between gap-[var(--space-3)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="mission">Mission Feed</Badge>
            <Badge variant={unreadCount > 0 ? "warning" : "success"}>
              {unreadCount > 0 ? "Unread activity" : "All caught up"}
            </Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[1.25rem] font-semibold leading-none text-[var(--text-primary)]">
              Notifications
            </span>
            <span className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Review alerts, approval prompts, and pane activity without leaving the current
              workspace context.
            </span>
          </div>
        </div>

        <div className="flex flex-wrap gap-[var(--space-2)]">
          <Button onClick={markAllRead} variant="secondary" size="sm" title="Mark all read">
            Read
          </Button>
          <Button onClick={clearAll} variant="destructive" size="sm" title="Clear all">
            Purge
          </Button>
          <Button onClick={close} variant="ghost" size="sm" title="Close">
            Close
          </Button>
        </div>
      </div>

      <div className="grid gap-[var(--space-3)] md:grid-cols-3">
        <MetricCard label="Unread" value={String(unreadCount)} tone={unreadCount > 0 ? "warning" : "success"} />
        <MetricCard label="Total" value={String(totalCount)} tone="default" />
        <MetricCard label="State" value={unreadCount > 0 ? "attention" : "quiet"} tone="mission" />
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
  tone: "default" | "warning" | "success" | "mission";
}) {
  const accentClasses =
    tone === "warning"
      ? "border-[var(--warning-border)]/80"
      : tone === "success"
        ? "border-[var(--success-border)]/80"
        : tone === "mission"
          ? "border-[var(--mission-border)]/80"
          : "";

  return (
    <div
      className={cn(
        panelSurfaceClassName,
        "grid gap-[var(--space-2)] rounded-[var(--radius-lg)] bg-[var(--panel)]/60 px-[var(--space-4)] py-[var(--space-3)] shadow-none",
        accentClasses
      )}
    >
      <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
        {label}
      </span>
      <span className="text-[var(--text-base)] font-semibold text-[var(--text-primary)]">{value}</span>
    </div>
  );
}
