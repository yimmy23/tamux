import type { TranscriptEntry } from "../../lib/types";
import { StaticLog } from "../StaticLog";
import { Badge, Button, cn, panelSurfaceClassName } from "../ui";
import { formatBytes, isTranscriptEntry, type TimelineEntry } from "./shared";

export function SessionVaultContent({
  timeline,
  timelineMode,
  setTimelineMode,
  selected,
  setSelectedId,
  display,
  workspaceLabels,
  surfaceLabels,
  runTimelineCommand,
  sendSelectedToActivePane,
  copySelected,
  removeTranscript,
  openSelectedFile,
  revealSelectedFile,
  exportSelected,
  timelineIndex,
  setTimelineIndex,
}: {
  timeline: TimelineEntry[];
  timelineMode: "timeline" | "transcripts";
  setTimelineMode: (mode: "timeline" | "transcripts") => void;
  selected: TranscriptEntry | null;
  setSelectedId: (id: string | null) => void;
  display: TranscriptEntry[];
  workspaceLabels: Map<string, string>;
  surfaceLabels: Map<string, string>;
  runTimelineCommand: (command: string, execute: boolean) => Promise<void>;
  sendSelectedToActivePane: (execute?: boolean) => Promise<void>;
  copySelected: () => Promise<void>;
  removeTranscript: (id: string) => void;
  openSelectedFile: () => Promise<void>;
  revealSelectedFile: () => Promise<void>;
  exportSelected: () => void;
  timelineIndex: number;
  setTimelineIndex: (value: number) => void;
}) {
  const scrubTarget = timeline[timelineIndex] ?? timeline[0] ?? null;

  return (
    <>
      <div className="grid gap-[var(--space-4)] border-b border-[var(--border-subtle)] bg-[var(--panel)]/35 px-[var(--space-5)] py-[var(--space-4)]">
        <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)]">
          <div className="grid gap-[var(--space-1)]">
            <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
              Time Travel Timeline
            </div>
            <div className="text-[var(--text-xs)] text-[var(--text-secondary)]">
              Rewind recent commands and transcript checkpoints for the current scope.
            </div>
          </div>
          <div className="flex gap-[var(--space-2)]">
            <Button
              type="button"
              onClick={() => setTimelineMode("timeline")}
              variant={timelineMode === "timeline" ? "primary" : "secondary"}
              size="sm"
            >
              Timeline
            </Button>
            <Button
              type="button"
              onClick={() => setTimelineMode("transcripts")}
              variant={timelineMode === "transcripts" ? "primary" : "secondary"}
              size="sm"
            >
              Transcripts
            </Button>
          </div>
        </div>

        <div className="flex gap-[var(--space-3)] overflow-x-auto pb-[var(--space-1)]">
          {timeline.length === 0 ? (
            <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">
              No timeline events in this scope.
            </div>
          ) : (
            timeline.map((entry) => {
              if (isTranscriptEntry(entry)) {
                const isSelected = selected?.id === entry.id;
                return (
                  <button
                    key={entry.id}
                    type="button"
                    onClick={() => {
                      setTimelineMode("transcripts");
                      setSelectedId(entry.id);
                    }}
                    className={cn(
                      panelSurfaceClassName,
                      "grid min-w-[14rem] max-w-[16rem] flex-shrink-0 gap-[var(--space-2)] rounded-[var(--radius-lg)] px-[var(--space-4)] py-[var(--space-3)] text-left shadow-none",
                      isSelected
                        ? "border-[var(--accent-border)] bg-[var(--accent-soft)]/50"
                        : "bg-[var(--card)]/80"
                    )}
                  >
                    <Badge variant="timeline">checkpoint</Badge>
                    <strong className="text-[var(--text-xs)] text-[var(--text-primary)]">{entry.reason}</strong>
                    <span className="text-[10px] text-[var(--text-muted)]">
                      {new Date(entry.capturedAt).toLocaleTimeString()}
                    </span>
                    <span className="line-clamp-3 text-[var(--text-xs)] leading-5 text-[var(--text-secondary)]">
                      {entry.preview || entry.filename}
                    </span>
                  </button>
                );
              }

              return (
                <div
                  key={entry.id}
                  className={cn(
                    panelSurfaceClassName,
                    "grid min-w-[14rem] max-w-[16rem] flex-shrink-0 gap-[var(--space-2)] rounded-[var(--radius-lg)] px-[var(--space-4)] py-[var(--space-3)] shadow-none"
                  )}
                >
                  <Badge variant="default">command</Badge>
                  <strong className="text-[var(--text-xs)] text-[var(--text-primary)]">
                    {entry.exitCode === null ? "running" : entry.exitCode === 0 ? "success" : `exit ${entry.exitCode}`}
                  </strong>
                  <span className="text-[10px] text-[var(--text-muted)]">
                    {new Date(entry.timestamp).toLocaleTimeString()}
                  </span>
                  <span className="line-clamp-3 text-[var(--text-xs)] leading-5 text-[var(--text-secondary)]">
                    {entry.command}
                  </span>
                  <div className="mt-[var(--space-1)] flex gap-[var(--space-2)]">
                    <Button type="button" size="sm" variant="secondary" onClick={() => void runTimelineCommand(entry.command, false)}>
                      Type
                    </Button>
                    <Button type="button" size="sm" variant="primary" onClick={() => void runTimelineCommand(entry.command, true)}>
                      Run
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto bg-[var(--panel)]/20">
        {timelineMode === "transcripts" && display.length === 0 ? (
          <EmptyState message="No transcripts captured yet" />
        ) : timelineMode === "timeline" ? (
          <div className="grid gap-[var(--space-3)] p-[var(--space-5)]">
            {timeline.length === 0 ? (
              <EmptyState message="No timeline events for the current filters." />
            ) : (
              timeline.map((entry) => {
                if (isTranscriptEntry(entry)) {
                  return (
                    <div
                      key={entry.id}
                      className={cn(
                        panelSurfaceClassName,
                        "flex gap-[var(--space-3)] rounded-[var(--radius-lg)] bg-[var(--card)]/80 px-[var(--space-4)] py-[var(--space-4)] shadow-none"
                      )}
                    >
                      <div className="w-[0.25rem] flex-shrink-0 rounded-full bg-[var(--timeline)]" />
                      <div className="flex-1">
                        <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)]">
                          <div>
                            <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
                              Checkpoint · {entry.reason}
                            </div>
                            <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
                              {entry.filename}
                            </div>
                          </div>
                          <Button type="button" size="sm" variant="secondary" onClick={() => setSelectedId(entry.id)}>
                            Inspect
                          </Button>
                        </div>
                        <div className="mt-[var(--space-3)] whitespace-pre-wrap text-[var(--text-xs)] leading-6 text-[var(--text-secondary)]">
                          {entry.preview}
                        </div>
                      </div>
                    </div>
                  );
                }

                const toneClass =
                  entry.exitCode === 0
                    ? "bg-[var(--success)]"
                    : entry.exitCode === null
                      ? "bg-[var(--accent)]"
                      : "bg-[var(--danger)]";

                return (
                  <div
                    key={entry.id}
                    className={cn(
                      panelSurfaceClassName,
                      "flex gap-[var(--space-3)] rounded-[var(--radius-lg)] bg-[var(--card)]/80 px-[var(--space-4)] py-[var(--space-4)] shadow-none"
                    )}
                  >
                    <div className={cn("w-[0.25rem] flex-shrink-0 rounded-full", toneClass)} />
                    <div className="flex-1">
                      <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)]">
                        <div>
                          <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
                            Command
                          </div>
                          <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
                            {entry.command}
                          </div>
                        </div>
                        <div className="flex gap-[var(--space-2)]">
                          <Button type="button" size="sm" variant="secondary" onClick={() => void runTimelineCommand(entry.command, false)}>
                            Type
                          </Button>
                          <Button type="button" size="sm" variant="primary" onClick={() => void runTimelineCommand(entry.command, true)}>
                            Run
                          </Button>
                        </div>
                      </div>
                      <div className="mt-[var(--space-3)] text-[var(--text-xs)] text-[var(--text-secondary)]">
                        {new Date(entry.timestamp).toLocaleString()} · {entry.cwd ?? "no cwd"}
                        {entry.durationMs !== null ? ` · ${entry.durationMs}ms` : ""}
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        ) : (
          <div className="grid min-h-[35rem] grid-cols-[26rem_minmax(0,1fr)]">
            <div className="overflow-auto border-r border-[var(--border-subtle)]">
              {display.map((tx) => (
                <button
                  key={tx.id}
                  type="button"
                  onClick={() => setSelectedId(tx.id)}
                  className={cn(
                    "flex w-full flex-col items-start gap-[var(--space-2)] border-b border-[var(--border-subtle)] px-[var(--space-4)] py-[var(--space-4)] text-left transition-colors hover:bg-[var(--muted)]/60",
                    selected?.id === tx.id ? "bg-[var(--accent-soft)]/40" : "bg-transparent"
                  )}
                >
                  <div className="flex w-full items-center justify-between gap-[var(--space-2)]">
                    <span className="truncate text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
                      {tx.filename}
                    </span>
                    <span className="text-[var(--text-xs)] text-[var(--text-muted)]">
                      {formatBytes(tx.sizeBytes)}
                    </span>
                  </div>
                  <div className="text-[var(--text-xs)] text-[var(--text-secondary)]">
                    {tx.reason}
                    {tx.cwd ? ` · ${tx.cwd}` : ""}
                  </div>
                  <div className="max-h-16 overflow-hidden whitespace-pre-wrap text-[var(--text-xs)] leading-5 text-[var(--text-secondary)]">
                    {tx.preview}
                  </div>
                </button>
              ))}
            </div>
            <div className="flex min-w-0 flex-col">
              {selected ? (
                <>
                  <div className="flex flex-wrap items-start justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] bg-[var(--card)]/80 px-[var(--space-4)] py-[var(--space-4)]">
                    <div className="min-w-0">
                      <div className="text-[var(--text-base)] font-semibold text-[var(--text-primary)]">
                        {selected.filename}
                      </div>
                      <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
                        {new Date(selected.capturedAt).toLocaleString()} · {selected.reason} · {formatBytes(selected.sizeBytes)}
                      </div>
                      <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
                        {(selected.workspaceId && workspaceLabels.get(selected.workspaceId)) || "No workspace"}
                        {selected.surfaceId ? ` / ${surfaceLabels.get(selected.surfaceId) ?? selected.surfaceId}` : ""}
                        {selected.paneId ? ` / ${selected.paneId}` : ""}
                      </div>
                    </div>
                    <div className="flex flex-wrap gap-[var(--space-2)]">
                      <Button onClick={() => void sendSelectedToActivePane(false)} variant="secondary" size="sm">
                        Type
                      </Button>
                      <Button onClick={() => void sendSelectedToActivePane(true)} size="sm">
                        Run
                      </Button>
                      <Button onClick={() => void copySelected()} variant="secondary" size="sm">
                        Copy All
                      </Button>
                      <Button onClick={() => removeTranscript(selected.id)} variant="destructive" size="sm">
                        Delete
                      </Button>
                      <Button onClick={() => void openSelectedFile()} variant="secondary" size="sm">
                        Open File
                      </Button>
                      <Button onClick={() => void revealSelectedFile()} variant="secondary" size="sm">
                        Open Folder
                      </Button>
                      <Button onClick={exportSelected} variant="secondary" size="sm">
                        Export
                      </Button>
                    </div>
                  </div>
                  <div className="min-h-0 flex-1">
                    <StaticLog content={selected.content} maxHeight="100%" />
                  </div>
                </>
              ) : null}
            </div>
          </div>
        )}
      </div>

      {timeline.length > 0 ? (
        <div className="grid gap-[var(--space-2)] border-t border-[var(--border-subtle)] bg-[var(--card)] px-[var(--space-5)] py-[var(--space-4)]">
          <input
            type="range"
            min={0}
            max={Math.max(0, timeline.length - 1)}
            value={Math.min(timelineIndex, Math.max(0, timeline.length - 1))}
            onChange={(event) => {
              const nextIndex = Number(event.target.value);
              setTimelineIndex(nextIndex);
              const target = timeline[nextIndex];
              if (target && isTranscriptEntry(target)) {
                setSelectedId(target.id);
              }
            }}
            className="w-full accent-[var(--timeline)]"
          />
          {scrubTarget ? (
            <div className="text-[var(--text-xs)] text-[var(--text-secondary)]">
              Scrub target:{" "}
              {isTranscriptEntry(scrubTarget)
                ? `${scrubTarget.reason} checkpoint`
                : scrubTarget.command}
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}

function EmptyState({ message }: { message: string }) {
  return (
    <div className="flex min-h-[14rem] items-center justify-center px-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
      {message}
    </div>
  );
}
