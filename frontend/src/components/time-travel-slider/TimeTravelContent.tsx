import { Badge, Button, cn, panelSurfaceClassName } from "../ui";
import type { SnapshotEntry } from "./shared";

export function TimeTravelContent({
  snapshots,
  selectedIndex,
  setSelectedIndex,
  confirmRestore,
  setConfirmRestore,
  isRestoring,
  handleRestore,
}: {
  snapshots: SnapshotEntry[];
  selectedIndex: number;
  setSelectedIndex: (value: number) => void;
  confirmRestore: boolean;
  setConfirmRestore: (value: boolean) => void;
  isRestoring: boolean;
  handleRestore: () => void;
}) {
  const selected = snapshots[selectedIndex] ?? null;

  if (snapshots.length === 0) {
    return (
      <div className="py-[var(--space-4)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
        No snapshots recorded yet. Snapshots are created before managed command execution.
      </div>
    );
  }

  return (
    <>
      <div className="flex items-center gap-[var(--space-2)] px-[var(--space-1)]">
        {snapshots.map((snapshot, index) => (
          <button
            key={snapshot.snapshot_id}
            onClick={() => {
              setSelectedIndex(index);
              setConfirmRestore(false);
            }}
            title={`${snapshot.label} — ${new Date(snapshot.created_at).toLocaleTimeString()}`}
            className={cn(
              "rounded-full border-0 transition-all duration-150 ease-out",
              index === selectedIndex ? "h-3 w-3 bg-[var(--timeline)]" : "h-2 w-2",
              index !== selectedIndex && snapshot.status === "ready"
                ? "bg-[var(--accent)]"
                : index !== selectedIndex
                  ? "bg-[var(--text-muted)]"
                  : ""
            )}
          />
        ))}
        <div className="h-[2px] flex-1 rounded-full bg-[var(--border-subtle)]" />
      </div>

      <input
        type="range"
        min={0}
        max={Math.max(0, snapshots.length - 1)}
        value={selectedIndex}
        onChange={(event) => {
          setSelectedIndex(Number(event.target.value));
          setConfirmRestore(false);
        }}
        className="w-full accent-[var(--timeline)]"
      />

      {selected ? (
        <div
          className={cn(
            panelSurfaceClassName,
            "flex flex-wrap items-end justify-between gap-[var(--space-4)] rounded-[var(--radius-lg)] bg-[var(--panel)]/50 px-[var(--space-4)] py-[var(--space-4)] shadow-none"
          )}
        >
          <div className="min-w-0 flex-1">
            <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
              {selected.label}
            </div>
            <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
              {new Date(selected.created_at).toLocaleString()}
              {selected.command ? (
                <span className="ml-[var(--space-2)] font-mono opacity-80">
                  {selected.command.length > 60 ? `${selected.command.slice(0, 60)}...` : selected.command}
                </span>
              ) : null}
            </div>
            <div className="mt-[var(--space-2)] flex flex-wrap gap-[var(--space-2)]">
              <Badge variant={selected.status === "ready" ? "success" : "warning"}>{selected.status}</Badge>
              <Badge variant="default">
                {selectedIndex + 1} / {snapshots.length}
              </Badge>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            {confirmRestore ? (
              <>
                <span className="text-[var(--text-xs)] text-[var(--warning)]">Overwrite workspace?</span>
                <Button onClick={handleRestore} variant="destructive" size="sm">
                  {isRestoring ? "Restoring..." : "Confirm"}
                </Button>
                <Button onClick={() => setConfirmRestore(false)} variant="secondary" size="sm">
                  Cancel
                </Button>
              </>
            ) : (
              <Button
                onClick={handleRestore}
                disabled={isRestoring || selected.status !== "ready"}
                variant="primary"
                size="sm"
              >
                Restore
              </Button>
            )}
          </div>
        </div>
      ) : null}
    </>
  );
}
