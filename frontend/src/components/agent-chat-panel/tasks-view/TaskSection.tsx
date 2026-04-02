import { useCallback, useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { ActionButton, iconButtonStyle } from "../shared";
import {
  fetchFilePreview,
  fetchGitDiff,
  fetchThreadWorkContext,
  type ThreadWorkContext,
} from "../../../lib/agentWorkContext";
import {
  formatTaskStatus,
  formatTaskTimestamp,
  isTaskActive,
  taskStatusColor,
  type AgentQueueTask,
} from "../../../lib/agentTaskQueue";
import { formatRunStatus, runStatusColor, type AgentRun } from "../../../lib/agentRuns";
import { shortenHomePath, useWorkspaceStore } from "../../../lib/workspaceStore";
import { detailLabelStyle } from "./styles";
import { findTaskWorkspaceLocation, taskLooksLikeCoding, workContextKindColor, workContextKindLabel } from "./helpers";
import type { TaskWorkspaceLocation, ThreadTarget } from "./types";

export function TaskCard({
  task,
  selected,
  onSelect,
  onCancel,
}: {
  task: AgentQueueTask;
  selected: boolean;
  onSelect: () => void;
  onCancel?: () => void;
}) {
  const statusColor = taskStatusColor(task.status);
  const isActive = isTaskActive(task);

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
      style={{
        width: "100%",
        textAlign: "left",
        padding: "var(--space-3)",
        borderRadius: "var(--radius-md)",
        border: selected ? `1px solid ${statusColor}` : "1px solid var(--border)",
        background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
        marginBottom: "var(--space-2)",
        cursor: "pointer",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: "var(--text-sm)", fontWeight: 500, color: "var(--text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {task.title}
          </div>
          {task.goal_run_title && (
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              Goal: {task.goal_run_title}
            </div>
          )}
          {task.source === "subagent" && (
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              Subagent · runtime {task.runtime ?? "daemon"}
            </div>
          )}
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
            <span style={{ color: statusColor, fontWeight: 600 }}>{formatTaskStatus(task)}</span>
            {task.status === "in_progress" && task.progress > 0 && <span> {task.progress}%</span>}
            <span style={{ marginLeft: "var(--space-2)" }}>{formatTaskTimestamp(task.created_at)}</span>
            {typeof task.retry_count === "number" && typeof task.max_retries === "number" && (
              <span style={{ marginLeft: "var(--space-2)" }}>
                retry {task.retry_count}/{task.max_retries === 0 ? "∞" : task.max_retries}
              </span>
            )}
          </div>
        </div>
        {isActive && onCancel && (
          <button type="button" onClick={(event) => { event.stopPropagation(); onCancel(); }} style={{ ...iconButtonStyle, fontSize: 11 }} title="Cancel task">
            Cancel
          </button>
        )}
      </div>
      {(task.blocked_reason || task.error) && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
          {task.blocked_reason ?? task.error}
        </div>
      )}
      {task.command && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
          {task.command}
        </div>
      )}
    </div>
  );
}

function TaskCodePreview({
  task,
  location,
}: {
  task: AgentQueueTask;
  location: TaskWorkspaceLocation | null;
}) {
  const [context, setContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [previewText, setPreviewText] = useState("");
  const [loadingEntries, setLoadingEntries] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const codingTask = taskLooksLikeCoding(task);
  const threadId = task.thread_id || null;
  const bridge = getBridge();
  const selectedEntry = useMemo(
    () => context.entries.find((entry) => entry.path === selectedPath) ?? null,
    [context.entries, selectedPath],
  );

  useEffect(() => {
    if (!threadId) {
      setContext({ threadId: "", entries: [] });
      setSelectedPath(null);
      setPreviewText("");
      setLoadingEntries(false);
      setLoadingDiff(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoadingEntries(true);
    setError(null);

    void fetchThreadWorkContext(threadId)
      .then((nextContext) => {
        if (cancelled) return;
        setContext(nextContext);
        setSelectedPath((current) => current && nextContext.entries.some((entry) => entry.path === current) ? current : nextContext.entries[0]?.path ?? null);
      })
      .catch((reason: unknown) => {
        if (cancelled) return;
        setContext({ threadId, entries: [] });
        setSelectedPath(null);
        setPreviewText("");
        setError(reason instanceof Error ? reason.message : String(reason));
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingEntries(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [task.id, threadId]);

  useEffect(() => {
    if (!threadId || !bridge?.onAgentEvent) {
      return;
    }
    return bridge.onAgentEvent((event: any) => {
      if (event?.type !== "work_context_update" || event?.thread_id !== threadId) {
        return;
      }
      void fetchThreadWorkContext(threadId).then((nextContext) => {
        setContext(nextContext);
        setSelectedPath((current) => current && nextContext.entries.some((entry) => entry.path === current) ? current : nextContext.entries[0]?.path ?? null);
      });
    });
  }, [bridge, threadId]);

  useEffect(() => {
    if (!selectedEntry) {
      setPreviewText("");
      setLoadingDiff(false);
      return;
    }

    let cancelled = false;
    setLoadingDiff(true);
    setError(null);

    const previewPromise = selectedEntry.repoRoot
      ? fetchGitDiff(selectedEntry.repoRoot, selectedEntry.path)
      : fetchFilePreview(selectedEntry.path).then((preview) => preview?.content ?? "");

    void previewPromise
      .then((output) => {
        if (!cancelled) {
          setPreviewText(output);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setPreviewText("");
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingDiff(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selectedEntry]);

  if (!threadId || (!codingTask && context.entries.length === 0 && !loadingEntries && !error)) {
    return null;
  }

  return (
    <div style={{ marginTop: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      <div style={detailLabelStyle}>Work Context</div>
      <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
        Scope: {location?.cwd ? shortenHomePath(location.cwd) : "thread workspace"}
        <span style={{ marginLeft: "var(--space-2)" }}>
          {loadingEntries ? "Refreshing..." : `${context.entries.length} file${context.entries.length === 1 ? "" : "s"} / artifact${context.entries.length === 1 ? "" : "s"}`}
        </span>
      </div>
      {error && <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>{error}</div>}
      {context.entries.length > 0 ? (
        <div style={{ display: "grid", gridTemplateColumns: "minmax(220px, 280px) minmax(0, 1fr)", gap: "var(--space-2)" }}>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)", maxHeight: 320, overflow: "auto" }}>
            {context.entries.map((entry) => {
              const selected = entry.path === selectedPath;
              return (
                <button
                  key={`${entry.source}:${entry.path}`}
                  type="button"
                  onClick={() => setSelectedPath(entry.path)}
                  style={{
                    textAlign: "left",
                    padding: "var(--space-2)",
                    borderRadius: "var(--radius-sm)",
                    border: selected ? `1px solid ${workContextKindColor(entry)}` : "1px solid var(--border)",
                    background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                    cursor: "pointer",
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", marginBottom: 4, flexWrap: "wrap" }}>
                    <span style={{ fontSize: "var(--text-xs)", color: workContextKindColor(entry), fontWeight: 600 }}>
                      {workContextKindLabel(entry)}
                    </span>
                    <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
                      {entry.source}
                    </span>
                  </div>
                  <div style={{ fontSize: "var(--text-xs)", color: "var(--text-primary)", fontFamily: "var(--font-mono)", wordBreak: "break-word" }}>
                    {entry.path}
                  </div>
                  {entry.previousPath && (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4, wordBreak: "break-word" }}>
                      from {entry.previousPath}
                    </div>
                  )}
                </button>
              );
            })}
          </div>
          <div style={{ minHeight: 220, maxHeight: 320, overflow: "auto", padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)", border: "1px solid var(--border)" }}>
            {loadingDiff ? (
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>Loading preview...</div>
            ) : previewText.trim() ? (
              <pre
                style={{
                  margin: 0,
                  fontSize: "var(--text-xs)",
                  lineHeight: 1.5,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-mono)",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                }}
              >
                {previewText}
              </pre>
            ) : (
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                No preview available for the selected item.
              </div>
            )}
          </div>
        </div>
      ) : (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
          {loadingEntries ? "Refreshing work context..." : "No file or artifact activity detected for this task yet."}
        </div>
      )}
    </div>
  );
}

export function TaskPostMortem({
  task,
  subagents,
  onSelectTask,
  onOpenTaskThread,
}: {
  task: AgentQueueTask;
  subagents: AgentRun[];
  onSelectTask: (taskId: string) => void;
  onOpenTaskThread: (task: ThreadTarget) => void;
}) {
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
  const focusCanvasPanel = useWorkspaceStore((state) => state.focusCanvasPanel);
  const logs = [...(task.logs ?? [])].slice(-8).reverse();
  const location = useMemo(
    () => findTaskWorkspaceLocation(workspaces, task.session_id),
    [task.session_id, workspaces],
  );
  const openTaskSession = useCallback(() => {
    if (!location) {
      return;
    }
    setActiveWorkspace(location.workspaceId);
    setActiveSurface(location.surfaceId);
    focusCanvasPanel(location.paneId, { storePreviousView: true });
    setActivePaneId(location.paneId);
  }, [focusCanvasPanel, location, setActivePaneId, setActiveSurface, setActiveWorkspace]);

  return (
    <div style={{ padding: "var(--space-3)", borderRadius: "var(--radius-md)", border: "1px solid var(--border)", background: "var(--bg-secondary)" }}>
      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{task.title}</div>
      {task.goal_run_title && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
          Goal: {task.goal_run_title}
          {task.goal_step_title && task.goal_step_title !== task.title ? ` · Step: ${task.goal_step_title}` : ""}
        </div>
      )}
      <div style={{ fontSize: "var(--text-xs)", color: taskStatusColor(task.status), marginTop: 4 }}>
        {formatTaskStatus(task)}
      </div>
      {task.command && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
          Command: {task.command}
        </div>
      )}
      {task.session_id && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
          Session: {task.session_id}
        </div>
      )}
      <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
        Runtime: {task.runtime ?? "daemon"}
      </div>
      {location && (
        <div style={{ marginTop: "var(--space-2)", display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            Workspace: {location.workspaceName} · Surface: {location.surfaceName}
            {location.cwd ? ` · ${shortenHomePath(location.cwd)}` : ""}
          </div>
          <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
            {task.thread_id && <ActionButton onClick={() => onOpenTaskThread(task)}>Open Chat</ActionButton>}
            <ActionButton onClick={openTaskSession}>Open Session</ActionButton>
          </div>
        </div>
      )}
      {!location && task.thread_id && (
        <div style={{ marginTop: "var(--space-2)", display: "flex", justifyContent: "flex-end" }}>
          <ActionButton onClick={() => onOpenTaskThread(task)}>Open Chat</ActionButton>
        </div>
      )}
      {task.dependencies && task.dependencies.length > 0 && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
          Depends on: {task.dependencies.join(", ")}
        </div>
      )}
      {task.parent_task_id && (
        <div style={{ marginTop: 4, display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
            Parent task: {task.parent_task_id}
          </div>
          <ActionButton onClick={() => onSelectTask(task.parent_task_id!)}>Back To Parent</ActionButton>
        </div>
      )}
      {task.last_error && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
          {task.last_error}
        </div>
      )}
      <TaskCodePreview task={task} location={location} />
      <div style={{ marginTop: "var(--space-3)" }}>
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: "var(--space-2)" }}>
          Subagents ({subagents.length})
        </div>
        {subagents.length > 0 ? (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            {subagents.map((subagent) => (
              <div key={subagent.id} style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)", border: `1px solid ${runStatusColor(subagent.status)}` }}>
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
                  <div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{subagent.title}</div>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2 }}>
                      {formatRunStatus(subagent)} · runtime {subagent.runtime ?? "daemon"}
                      {subagent.classification ? ` · ${subagent.classification}` : ""}
                      {subagent.session_id ? ` · session ${subagent.session_id}` : ""}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                    {subagent.thread_id && (
                      <ActionButton onClick={() => onOpenTaskThread(subagent)}>Open Chat</ActionButton>
                    )}
                    <ActionButton onClick={() => onSelectTask(subagent.id)}>Inspect</ActionButton>
                  </div>
                </div>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-2)" }}>
                  {subagent.description}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            No child subagents have been spawned for this task.
          </div>
        )}
      </div>
      <div style={{ marginTop: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
        {logs.length > 0 ? logs.map((log) => (
          <div key={log.id} style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
            <div style={{ fontSize: "var(--text-xs)", color: log.level === "error" ? "var(--danger)" : log.level === "warn" ? "var(--warning)" : "var(--text-muted)" }}>
              {log.phase} · attempt {log.attempt || 0} · {formatTaskTimestamp(log.timestamp)}
            </div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{log.message}</div>
            {log.details && (
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>{log.details}</div>
            )}
          </div>
        )) : (
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>No task logs recorded yet.</div>
        )}
      </div>
    </div>
  );
}
