import { useMemo, useState, type CSSProperties } from "react";
import { cn, overlayClassName, panelSurfaceClassName } from "./ui";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { getTerminalController } from "../lib/terminalRegistry";
import { CommandLogFilters } from "./command-log-panel/CommandLogFilters";
import { CommandLogHeader } from "./command-log-panel/CommandLogHeader";
import { CommandLogTable } from "./command-log-panel/CommandLogTable";
import { filterCommandEntries } from "./command-log-panel/shared";

/**
 * Filterable command log panel (Ctrl+Shift+L).
 */
type CommandLogPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function CommandLogPanel({ style, className }: CommandLogPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.commandLogOpen);
  const toggle = useWorkspaceStore((s) => s.toggleCommandLog);
  const entries = useCommandLogStore((s) => s.entries);
  const clearAll = useCommandLogStore((s) => s.clearAll);
  const removeEntry = useCommandLogStore((s) => s.removeEntry);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());

  const [query, setQuery] = useState("");
  const [workspaceFilter, setWorkspaceFilter] = useState("all");
  const [surfaceFilter, setSurfaceFilter] = useState("all");
  const [paneFilter, setPaneFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState("all");

  const workspaceLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const workspace of workspaces) {
      labels.set(workspace.id, workspace.name);
    }
    return labels;
  }, [workspaces]);

  const surfaceLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const workspace of workspaces) {
      for (const surface of workspace.surfaces) {
        labels.set(surface.id, surface.name);
      }
    }
    return labels;
  }, [workspaces]);

  const filteredEntries = filterCommandEntries(entries, {
    query,
    workspaceFilter,
    surfaceFilter,
    paneFilter,
    dateFilter,
    statusFilter,
  });

  const uniquePaneIds = [...new Set(entries.map((entry) => entry.paneId).filter(Boolean))] as string[];
  const workspaceOptions = [...workspaceLabels.entries()].map(([id, name]) => ({ id, name }));
  const surfaceOptions = [...surfaceLabels.entries()].map(([id, name]) => ({ id, name }));

  const copyCommand = async (command: string) => {
    await navigator.clipboard.writeText(command);
  };

  const sendToActivePane = async (command: string, execute: boolean) => {
    const controller = getTerminalController(activePaneId);
    if (!controller) return;
    await controller.sendText(command, { execute, trackHistory: execute });
  };

  const exportVisible = () => {
    const blob = new Blob([JSON.stringify(filteredEntries, null, 2)], {
      type: "application/json;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `amux-command-log-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  if (!open) return null;

  return (
    <div
      onClick={toggle}
      style={style}
      className={cn(
        overlayClassName,
        "fixed inset-0 z-[930] flex items-start justify-center px-[2vw] py-[4vh]",
        className
      )}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={cn(
          panelSurfaceClassName,
          "flex max-h-[88vh] w-[min(1380px,96vw)] flex-col overflow-hidden rounded-[var(--radius-xl)] border-[var(--border-strong)] bg-[var(--card)]"
        )}
      >
        <CommandLogHeader
          visibleCount={filteredEntries.length}
          totalCount={entries.length}
          failureCount={entries.filter((entry) => entry.exitCode !== null && entry.exitCode !== 0).length}
          runningCount={entries.filter((entry) => entry.exitCode === null).length}
          exportVisible={exportVisible}
          clearAll={clearAll}
          close={toggle}
        />

        <CommandLogFilters
          query={query}
          setQuery={setQuery}
          workspaceFilter={workspaceFilter}
          setWorkspaceFilter={setWorkspaceFilter}
          surfaceFilter={surfaceFilter}
          setSurfaceFilter={setSurfaceFilter}
          paneFilter={paneFilter}
          setPaneFilter={setPaneFilter}
          statusFilter={statusFilter}
          setStatusFilter={setStatusFilter}
          dateFilter={dateFilter}
          setDateFilter={setDateFilter}
          workspaceOptions={workspaceOptions}
          surfaceOptions={surfaceOptions}
          paneOptions={uniquePaneIds}
          close={toggle}
        />

        <div className="min-h-0 flex-1 overflow-auto bg-[var(--panel)]/35">
          <CommandLogTable
            entries={filteredEntries}
            workspaceLabels={workspaceLabels}
            surfaceLabels={surfaceLabels}
            copyCommand={copyCommand}
            sendToActivePane={sendToActivePane}
            removeEntry={removeEntry}
          />
        </div>
      </div>
    </div>
  );
}
