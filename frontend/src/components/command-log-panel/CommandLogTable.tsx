import type { CommandLogEntry } from "../../lib/types";
import { Badge, Button } from "../ui";

export function CommandLogTable({
  entries,
  workspaceLabels,
  surfaceLabels,
  copyCommand,
  sendToActivePane,
  removeEntry,
}: {
  entries: CommandLogEntry[];
  workspaceLabels: Map<string, string>;
  surfaceLabels: Map<string, string>;
  copyCommand: (command: string) => Promise<void>;
  sendToActivePane: (command: string, execute: boolean) => Promise<void>;
  removeEntry: (id: string) => void;
}) {
  if (entries.length === 0) {
    return (
      <div className="flex h-full min-h-[14rem] items-center justify-center px-[var(--space-8)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
        No commands logged for the current filters.
      </div>
    );
  }

  return (
    <table className="w-full border-collapse text-left text-[var(--text-xs)] text-[var(--text-primary)]">
      <thead className="sticky top-0 z-[1] bg-[var(--panel)]/95 backdrop-blur-[var(--panel-blur)]">
        <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Workspace</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Surface</th>
          <th className="px-[var(--space-4)] py-[var(--space-3)] font-medium">Command</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Path</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Exit</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Duration</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">CWD</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Time</th>
          <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Actions</th>
        </tr>
      </thead>
      <tbody>
        {entries.map((entry) => (
          <tr
            key={entry.id}
            className="border-b border-[var(--border-subtle)]/70 transition-colors hover:bg-[var(--muted)]/60"
          >
            <td className="px-[var(--space-3)] py-[var(--space-3)] whitespace-nowrap text-[var(--text-secondary)]">
              {entry.workspaceId ? (workspaceLabels.get(entry.workspaceId) ?? entry.workspaceId) : "-"}
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)] whitespace-nowrap text-[var(--text-secondary)]">
              {entry.surfaceId ? (surfaceLabels.get(entry.surfaceId) ?? entry.surfaceId) : "-"}
            </td>
            <td className="max-w-[32rem] px-[var(--space-4)] py-[var(--space-3)] font-mono text-[var(--text-xs)]">
              <div className="truncate" title={entry.command}>
                {entry.command}
              </div>
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)] whitespace-nowrap text-[var(--text-secondary)]">
              {entry.path ?? "-"}
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)]">
              <ExitBadge exitCode={entry.exitCode} />
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)]">
              {entry.durationMs !== null
                ? entry.durationMs < 1000
                  ? `${entry.durationMs}ms`
                  : `${(entry.durationMs / 1000).toFixed(1)}s`
                : "–"}
            </td>
            <td className="max-w-[10rem] px-[var(--space-3)] py-[var(--space-3)] text-[var(--text-secondary)]">
              <div className="truncate" title={entry.cwd ?? undefined}>
                {entry.cwd ?? "–"}
              </div>
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)] whitespace-nowrap text-[var(--text-secondary)]">
              {new Date(entry.timestamp).toLocaleString()}
            </td>
            <td className="px-[var(--space-3)] py-[var(--space-3)]">
              <div className="flex flex-wrap gap-[var(--space-2)]">
                <Button size="sm" variant="secondary" onClick={() => void copyCommand(entry.command)}>
                  Copy
                </Button>
                <Button size="sm" variant="outline" onClick={() => void sendToActivePane(entry.command, false)}>
                  Insert
                </Button>
                <Button size="sm" variant="primary" onClick={() => void sendToActivePane(entry.command, true)}>
                  Run
                </Button>
                <Button size="sm" variant="destructive" onClick={() => removeEntry(entry.id)}>
                  Delete
                </Button>
              </div>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ExitBadge({ exitCode }: { exitCode: number | null }) {
  if (exitCode === null) {
    return <Badge variant="warning">Running</Badge>;
  }
  if (exitCode === 0) {
    return <Badge variant="success">0</Badge>;
  }
  return <Badge variant="danger">{String(exitCode)}</Badge>;
}
