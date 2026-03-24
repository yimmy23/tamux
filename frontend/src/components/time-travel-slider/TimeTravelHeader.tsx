import { Badge, Button } from "../ui";

export function TimeTravelHeader({
  snapshotCount,
  onRefresh,
  toggle,
}: {
  snapshotCount: number;
  onRefresh: () => void;
  toggle: () => void;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-[var(--space-3)]">
      <div className="grid gap-[var(--space-2)]">
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <Badge variant="timeline">Time Travel</Badge>
          <Badge variant="default">
            {snapshotCount} snapshot{snapshotCount !== 1 ? "s" : ""}
          </Badge>
        </div>
        <div className="grid gap-[var(--space-1)]">
          <span className="text-[var(--text-base)] font-semibold text-[var(--text-primary)]">
            Filesystem Checkpoints
          </span>
          <span className="text-[var(--text-xs)] leading-5 text-[var(--text-secondary)]">
            Scrub through managed snapshots before restoring a previous workspace state.
          </span>
        </div>
      </div>
      <div className="flex gap-[var(--space-2)]">
        <Button onClick={onRefresh} variant="secondary" size="sm" title="Refresh snapshots">
          Refresh
        </Button>
        <Button onClick={toggle} variant="ghost" size="sm" title="Close (Esc)">
          Close
        </Button>
      </div>
    </div>
  );
}
