import { useState, useRef, useEffect, type CSSProperties } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { getTerminalController } from "../lib/terminalRegistry";
import { Badge, Card, Input, ScrollArea, Separator, Sheet, SheetContent } from "./ui";

/**
 * Command history picker (Ctrl+Alt+H).
 * Quick-search through recent unique commands and paste into active terminal.
 */
type CommandHistoryPickerProps = {
  style?: CSSProperties;
  className?: string;
};

export function CommandHistoryPicker({ style, className }: CommandHistoryPickerProps = {}) {
  const open = useWorkspaceStore((s) => s.commandHistoryOpen);
  const toggle = useWorkspaceStore((s) => s.toggleCommandHistory);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const getRecentEntries = useCommandLogStore((s) => s.getRecentEntries);

  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  const recent = getRecentEntries(200);
  const filtered = query.trim()
    ? recent.filter((entry) =>
      entry.command.toLowerCase().includes(query.toLowerCase()) ||
      (entry.cwd ?? "").toLowerCase().includes(query.toLowerCase()),
    )
    : recent;

  useEffect(() => {
    setSelectedIndex((current) => {
      if (filtered.length === 0) return 0;
      return Math.min(current, filtered.length - 1);
    });
  }, [filtered.length]);

  if (!open) return null;

  const selectCommand = async (cmd: string, execute = true) => {
    const controller = getTerminalController(activePaneId);

    if (controller) {
      await controller.sendText(cmd, { execute, trackHistory: execute });
    } else {
      await navigator.clipboard.writeText(cmd);
    }

    toggle();
  };

  return (
    <Sheet open={open} onOpenChange={(nextOpen) => !nextOpen && toggle()}>
      <SheetContent
        side="top"
        className="border-none bg-transparent p-0 shadow-none"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          inputRef.current?.focus();
        }}
      >
        <div className="flex justify-center px-[var(--space-4)] pt-16 pb-[var(--space-6)]">
          <Card
            style={style}
            className={[
              "flex h-[min(72vh,48rem)] w-[min(92vw,56rem)] flex-col overflow-hidden",
              className ?? "",
            ].join(" ")}
          >
            <div className="flex flex-col gap-[var(--space-3)] p-[var(--space-4)] pr-[calc(var(--space-4)+var(--space-6))]">
              <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                <Badge variant="agent">Command History</Badge>
                <Badge variant="default">{filtered.length} results</Badge>
              </div>
              <div className="text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">
                Recall a recent terminal command
              </div>
            </div>
            <Separator />
            <div className="px-[var(--space-4)] py-[var(--space-3)]">
              <Input
                ref={inputRef}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    e.stopPropagation();
                    toggle();
                  }
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setSelectedIndex((current) => Math.min(current + 1, filtered.length - 1));
                  }
                  if (e.key === "ArrowUp") {
                    e.preventDefault();
                    setSelectedIndex((current) => Math.max(current - 1, 0));
                  }
                  if (e.key === "Enter" && filtered.length > 0) {
                    void selectCommand(filtered[selectedIndex]?.command ?? filtered[0].command, !e.shiftKey);
                  }
                }}
                placeholder="Search command history by command, cwd, exit code..."
              />
            </div>
            <Separator />
            <ScrollArea className="min-h-0 flex-1">
              <div className="flex flex-col gap-[var(--space-2)] p-[var(--space-3)]">
                {filtered.length === 0 ? (
                  <Card className="border-dashed bg-[var(--surface)]/60 p-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
                    {query ? "No matching commands" : "No command history yet"}
                  </Card>
                ) : (
                  filtered.map((entry, i) => (
                    <button
                      key={`${entry.id}-${i}`}
                      type="button"
                      onClick={() => void selectCommand(entry.command, true)}
                      onMouseEnter={() => setSelectedIndex(i)}
                      className={[
                        "w-full rounded-[var(--radius-lg)] border p-[var(--space-3)] text-left transition-colors duration-100 ease-out",
                        selectedIndex === i
                          ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                          : "border-[var(--border)] bg-[var(--card)] hover:border-[var(--border-strong)] hover:bg-[var(--surface)]",
                      ].join(" ")}
                    >
                      <div className="flex flex-col gap-[var(--space-2)]">
                        <div className="truncate font-mono text-[13px] text-[var(--text-primary)]">{entry.command}</div>
                        <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)] text-[11px] text-[var(--text-secondary)]">
                          <span className="min-w-0 flex-1 truncate">{entry.cwd ?? "No cwd"}</span>
                          <Badge variant={entry.exitCode === null ? "warning" : entry.exitCode === 0 ? "success" : "danger"}>
                            {entry.exitCode === null ? "running" : `exit ${entry.exitCode}`}
                            {entry.durationMs !== null
                              ? ` · ${entry.durationMs < 1000 ? `${entry.durationMs}ms` : `${(entry.durationMs / 1000).toFixed(1)}s`}`
                              : ""}
                          </Badge>
                        </div>
                        <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)] text-[10px] text-[var(--text-muted)]">
                          <span>{entry.paneId ?? "No pane"}</span>
                          <span>{new Date(entry.timestamp).toLocaleString()}</span>
                        </div>
                      </div>
                    </button>
                  ))
                )}
              </div>
            </ScrollArea>
            <Separator />
            <div className="px-[var(--space-4)] py-[var(--space-3)] text-[var(--text-xs)] text-[var(--text-secondary)]">
              Up/Down selects. Enter runs. Shift+Enter types without running.
            </div>
          </Card>
        </div>
      </SheetContent>
    </Sheet>
  );
}
