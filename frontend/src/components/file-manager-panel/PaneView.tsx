import { Badge, Button, Input, cn, panelSurfaceClassName } from "../ui";
import { formatBytes, getParentPath, type FsEntry, type PaneState } from "./shared";

export function PaneView({
  title,
  pane,
  active,
  inputPath,
  onPathInputChange,
  onGo,
  onSelect,
  onOpen,
  onParent,
}: {
  title: string;
  pane: PaneState;
  active: boolean;
  inputPath: string;
  onPathInputChange: (value: string) => void;
  onGo: () => void;
  onSelect: (path: string | null) => void;
  onOpen: (entry: FsEntry) => void;
  onParent: () => void;
}) {
  const parentPath = getParentPath(pane.path);

  return (
    <div
      className={cn(
        panelSurfaceClassName,
        "flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[var(--radius-lg)] bg-[var(--card)] shadow-none",
        active ? "border-[var(--accent-border)]" : "border-[var(--border)]"
      )}
      onClick={() => onSelect(pane.selectedPath)}
    >
      <div className="grid gap-[var(--space-2)] border-b border-[var(--border-subtle)] bg-[var(--panel)]/35 p-[var(--space-3)]">
        <div className="flex items-center justify-between gap-[var(--space-2)]">
          <strong className="text-[var(--text-sm)] text-[var(--text-primary)]">{title} Pane</strong>
          {pane.loading ? <Badge variant="timeline">loading</Badge> : <Badge variant={active ? "accent" : "default"}>{active ? "active" : "idle"}</Badge>}
        </div>

        <div className="flex gap-[var(--space-2)]">
          <Input
            value={inputPath}
            onChange={(event) => onPathInputChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                onGo();
              }
            }}
          />
          <Button type="button" variant="secondary" size="sm" onClick={onGo}>
            Go
          </Button>
          <Button type="button" variant="secondary" size="sm" onClick={onParent} disabled={!parentPath}>
            Up
          </Button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {pane.error ? (
          <div className="px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-xs)] text-[var(--danger)]">
            {pane.error}
          </div>
        ) : null}

        <table className="w-full table-fixed border-collapse text-left text-[var(--text-xs)]">
          <thead className="sticky top-0 z-[1] bg-[var(--panel)]/95 backdrop-blur-[var(--panel-blur)]">
            <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
              <th className="px-[var(--space-3)] py-[var(--space-2)] font-medium">Name</th>
              <th className="px-[var(--space-3)] py-[var(--space-2)] font-medium">Size</th>
              <th className="px-[var(--space-3)] py-[var(--space-2)] font-medium">Modified</th>
            </tr>
          </thead>
          <tbody>
            {parentPath ? (
              <tr
                onClick={() => onSelect(parentPath)}
                onDoubleClick={onParent}
                className={rowClassName(pane.selectedPath === parentPath)}
              >
                <td className={cellClassName}>..</td>
                <td className={cellClassName}>-</td>
                <td className={cellClassName}>parent</td>
              </tr>
            ) : null}

            {pane.entries.map((entry) => (
              <tr
                key={entry.path}
                onClick={() => onSelect(entry.path)}
                onDoubleClick={() => onOpen(entry)}
                className={rowClassName(pane.selectedPath === entry.path)}
              >
                <td className={cellClassName} title={entry.path}>
                  {entry.isDirectory ? "[DIR] " : ""}
                  {entry.name}
                </td>
                <td className={cellClassName}>{entry.isDirectory ? "-" : formatBytes(entry.sizeBytes)}</td>
                <td className={cellClassName}>
                  {entry.modifiedAt ? new Date(entry.modifiedAt).toLocaleString() : "-"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const cellClassName =
  "truncate border-b border-[var(--border-subtle)]/70 px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-primary)]";

function rowClassName(selected: boolean) {
  return cn(
    "cursor-pointer transition-colors hover:bg-[var(--muted)]/60",
    selected ? "bg-[var(--accent-soft)]/40" : "bg-transparent"
  );
}
