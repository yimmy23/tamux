import { useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentMessage, AgentThread, AgentTodoItem } from "@/lib/agentStore";
import { fetchFilePreview, fetchGitDiff, fetchThreadWorkContext, type ThreadWorkContext, type WorkContextEntry } from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { shortenHomePath } from "@/lib/workspaceStore";
import { workContextKindColor, workContextKindLabel } from "@/components/agent-chat-panel/tasks-view/helpers";
import {
  previewRequestForWorkContextEntry,
  threadContextEntryDisplayPath,
  threadContextEntryKey,
} from "./threadContextPreview";
import { SpawnedContext } from "./ThreadsSpawnedContext";

type ContextTab = "todos" | "files" | "spawned";

export function ThreadsContext() {
  const runtime = useAgentChatPanelRuntime();
  const activeThread = runtime.activeThread;
  const [activeTab, setActiveTab] = useState<ContextTab>("todos");
  const [workContext, setWorkContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });
  const daemonThreadId = activeThread?.daemonThreadId ?? null;
  const spawnedCount = useMemo(
    () => countSpawnedNodes(runtime.spawnedAgentTree),
    [runtime.spawnedAgentTree],
  );
  const contextWindowTokens = resolveThreadContextWindowTokens(activeThread, runtime.agentSettings);
  const currentContextTokens = resolveCurrentContextTokens(activeThread, runtime.messages);

  useEffect(() => {
    if (!daemonThreadId || !activeThread) {
      setWorkContext({ threadId: "", entries: [] });
      return;
    }

    let cancelled = false;
    const requestThreadId = activeThread.daemonThreadId ?? daemonThreadId;
    void fetchThreadWorkContext(requestThreadId).then((next) => {
      if (!cancelled) {
        setWorkContext(next);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [activeThread, daemonThreadId]);

  useEffect(() => {
    const bridge = getBridge();
    if (!daemonThreadId || !bridge?.onAgentEvent) {
      return;
    }

    return bridge.onAgentEvent((event: any) => {
      if (event?.type !== "work_context_update" || event?.thread_id !== daemonThreadId) {
        return;
      }
      void fetchThreadWorkContext(daemonThreadId).then(setWorkContext);
    });
  }, [daemonThreadId]);

  return (
    <div className="zorai-thread-context-stack">
      <ContextWindowSummary
        currentTokens={currentContextTokens}
        contextWindowTokens={contextWindowTokens}
        pinnedCount={runtime.pinnedMessages.length}
      />

      <div className="zorai-context-tabs" role="tablist" aria-label="Thread context">
        <ContextTabButton id="todos" label="Todos" count={runtime.todos.length} activeTab={activeTab} onSelect={setActiveTab} />
        <ContextTabButton id="files" label="Files" count={workContext.entries.length} activeTab={activeTab} onSelect={setActiveTab} />
        <ContextTabButton id="spawned" label="Spawned" count={spawnedCount} activeTab={activeTab} onSelect={setActiveTab} />
      </div>

      {activeTab === "todos" ? <TodoContext todos={runtime.todos} /> : null}
      {activeTab === "files" ? <FilesContext entries={workContext.entries} /> : null}
      {activeTab === "spawned" ? (
        <SpawnedContext
          tree={runtime.spawnedAgentTree}
          selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
          canGoBackThread={runtime.canGoBackThread}
          threadNavigationDepth={runtime.threadNavigationDepth}
          backThreadTitle={runtime.backThreadTitle}
          canOpenSpawnedThread={runtime.canOpenSpawnedThread}
          openSpawnedThread={runtime.openSpawnedThread}
          goBackThread={runtime.goBackThread}
        />
      ) : null}

      {runtime.pinnedMessages.length > 0 ? (
        <PinnedThreadContext
          messages={runtime.pinnedMessages}
          onJumpToMessage={(messageId) => {
            document.getElementById(`zorai-message-${messageId}`)?.scrollIntoView({ block: "center", behavior: "smooth" });
          }}
          onUnpinMessage={(messageId) => runtime.activeThreadId
            ? runtime.unpinMessageForCompaction(runtime.activeThreadId, messageId)
            : undefined}
        />
      ) : null}
    </div>
  );
}

function ContextWindowSummary({
  currentTokens,
  contextWindowTokens,
  pinnedCount,
}: {
  currentTokens: number;
  contextWindowTokens: number;
  pinnedCount: number;
}) {
  const utilization = contextWindowTokens > 0
    ? Math.min(100, Math.round((currentTokens / contextWindowTokens) * 100))
    : 0;

  return (
    <section className="zorai-context-window-summary">
      <div>
        <div className="zorai-section-label">Context Window</div>
        <strong>{formatTokens(currentTokens)} / {formatTokens(contextWindowTokens)} tokens</strong>
      </div>
      <div className="zorai-context-window-meter" aria-label={`${utilization}% context used`}>
        <span style={{ width: `${utilization}%` }} />
      </div>
      <div className="zorai-context-window-meta">
        <span>{utilization}% used</span>
        <span>{pinnedCount} pinned</span>
      </div>
    </section>
  );
}

function ContextTabButton({
  id,
  label,
  count,
  activeTab,
  onSelect,
}: {
  id: ContextTab;
  label: string;
  count: number;
  activeTab: ContextTab;
  onSelect: (tab: ContextTab) => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={activeTab === id}
      className={activeTab === id ? "zorai-context-tab zorai-context-tab--active" : "zorai-context-tab"}
      onClick={() => onSelect(id)}
    >
      <span>{label}</span>
      <b>{count}</b>
    </button>
  );
}

function TodoContext({ todos }: { todos: AgentTodoItem[] }) {
  if (todos.length === 0) {
    return <div className="zorai-empty">No todos for this thread yet.</div>;
  }

  return (
    <section className="zorai-context-list">
      {todos.map((todo) => (
        <article key={todo.id} className="zorai-context-list-item">
          <div>
            <strong>{todo.content}</strong>
            <span>{todo.status.replace(/_/g, " ")}</span>
          </div>
        </article>
      ))}
    </section>
  );
}

function FilesContext({ entries }: { entries: WorkContextEntry[] }) {
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [previewText, setPreviewText] = useState("");
  const [previewKind, setPreviewKind] = useState<"git-diff" | "file-preview" | null>(null);
  const [previewTitle, setPreviewTitle] = useState("");
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const selectedEntry = useMemo(
    () => entries.find((entry) => threadContextEntryKey(entry) === selectedKey) ?? null,
    [entries, selectedKey],
  );

  useEffect(() => {
    setSelectedKey((current) => {
      if (current && entries.some((entry) => threadContextEntryKey(entry) === current)) {
        return current;
      }
      return entries[0] ? threadContextEntryKey(entries[0]) : null;
    });
  }, [entries]);

  useEffect(() => {
    if (!selectedEntry) {
      setPreviewText("");
      setPreviewKind(null);
      setPreviewTitle("");
      setLoadingPreview(false);
      setPreviewError(null);
      return;
    }

    let cancelled = false;
    const request = previewRequestForWorkContextEntry(selectedEntry);
    setLoadingPreview(true);
    setPreviewError(null);
    setPreviewKind(request.type);
    setPreviewTitle(request.type === "git-diff" ? "Git diff" : "File preview");

    const previewPromise = request.type === "git-diff"
      ? fetchGitDiff(request.repoRoot, request.filePath)
      : fetchFilePreview(request.path).then((preview) => {
        if (!preview) return "";
        if (!preview.isText) return "Binary file preview is not available.";
        return preview.truncated ? `${preview.content}\n\n[Preview truncated]` : preview.content;
      });

    void previewPromise
      .then((output) => {
        if (!cancelled) {
          setPreviewText(output);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setPreviewText("");
          setPreviewError(reason instanceof Error ? reason.message : String(reason));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingPreview(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selectedEntry]);

  if (entries.length === 0) {
    return <div className="zorai-empty">No file or artifact context recorded for this thread yet.</div>;
  }

  return (
    <section className="zorai-file-context">
      <div className="zorai-file-context__list">
        {entries.slice(0, 24).map((entry) => {
          const entryKey = threadContextEntryKey(entry);
          const selected = entryKey === selectedKey;
          return (
            <button
              key={entryKey}
              type="button"
              className={selected ? "zorai-file-context__item zorai-file-context__item--active" : "zorai-file-context__item"}
              onClick={() => setSelectedKey(entryKey)}
            >
              <span style={{ color: workContextKindColor(entry) }}>{workContextKindLabel(entry)}</span>
              <strong>{threadContextEntryDisplayPath(entry, shortenHomePath)}</strong>
              {entry.previousPath ? <small>from {shortenHomePath(entry.previousPath)}</small> : null}
            </button>
          );
        })}
      </div>
      <div className="zorai-file-preview">
        <div className="zorai-file-preview__header">
          <div>
            <div className="zorai-section-label">{previewTitle || "Preview"}</div>
            <strong>{selectedEntry ? threadContextEntryDisplayPath(selectedEntry, shortenHomePath) : "Select a file"}</strong>
          </div>
        </div>
        {previewError ? <div className="zorai-empty zorai-empty--danger">{previewError}</div> : null}
        {loadingPreview ? (
          <div className="zorai-empty">Loading preview...</div>
        ) : previewText.trim() ? (
          <PreviewText text={previewText} kind={previewKind} />
        ) : (
          <div className="zorai-empty">
            {previewKind === "git-diff" ? "No diff preview available for the selected file." : "No preview available for the selected file."}
          </div>
        )}
      </div>
    </section>
  );
}

function PreviewText({ text, kind }: { text: string; kind: "git-diff" | "file-preview" | null }) {
  if (kind !== "git-diff") {
    return <pre className="zorai-file-preview__pre">{text}</pre>;
  }

  return (
    <pre className="zorai-file-preview__pre">
      {text.split("\n").map((line, index) => {
        const lineClass = line.startsWith("+") && !line.startsWith("+++")
          ? "zorai-diff-line zorai-diff-line--added"
          : line.startsWith("-") && !line.startsWith("---")
            ? "zorai-diff-line zorai-diff-line--removed"
            : line.startsWith("@@")
              ? "zorai-diff-line zorai-diff-line--hunk"
              : "zorai-diff-line";
        return <span key={`${index}:${line}`} className={lineClass}>{line || " "}</span>;
      })}
    </pre>
  );
}


function PinnedThreadContext({
  messages,
  onJumpToMessage,
  onUnpinMessage,
}: {
  messages: AgentMessage[];
  onJumpToMessage: (messageId: string) => void;
  onUnpinMessage: (messageId: string) => void | Promise<unknown> | undefined;
}) {
  return (
    <section className="zorai-pinned-context">
      <div>
        <div className="zorai-section-label">Pinned Messages</div>
      </div>
      {messages.map((message) => (
        <article key={message.id} className="zorai-pinned-message">
          <div>
            <strong>{message.role}</strong>
            <span>{formatTime(message.createdAt)}</span>
          </div>
          <p>{message.content || summarizePinnedMessage(message)}</p>
          <div className="zorai-card-actions">
            <button type="button" className="zorai-ghost-button" onClick={() => onJumpToMessage(message.id)}>
              Jump
            </button>
            <button type="button" className="zorai-ghost-button" onClick={() => void onUnpinMessage(message.id)}>
              Unpin
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}

function resolveThreadContextWindowTokens(thread: AgentThread | undefined, agentSettings: any): number {
  if (typeof thread?.profileContextWindowTokens === "number" && thread.profileContextWindowTokens > 0) {
    return Math.trunc(thread.profileContextWindowTokens);
  }

  const activeProviderId = typeof agentSettings.active_provider === "string" ? agentSettings.active_provider : null;
  const activeProvider = activeProviderId ? agentSettings[activeProviderId] : null;
  const providerWindow = typeof activeProvider?.context_window_tokens === "number" ? activeProvider.context_window_tokens : null;
  const fallbackWindow = typeof agentSettings.context_window_tokens === "number" ? agentSettings.context_window_tokens : 0;
  return Math.max(1, Math.trunc(providerWindow ?? fallbackWindow ?? 1));
}

function resolveCurrentContextTokens(thread: AgentThread | undefined, messages: AgentMessage[]): number {
  if (typeof thread?.activeContextWindowTokens === "number" && thread.activeContextWindowTokens >= 0) {
    return Math.trunc(thread.activeContextWindowTokens);
  }

  return messages.reduce((sum, message) => sum + Math.max(0, Math.trunc(message.totalTokens || 0)), 0);
}

function countSpawnedNodes(tree: ReturnType<typeof useAgentChatPanelRuntime>["spawnedAgentTree"]): number {
  if (!tree) return 0;
  const countNode = (node: NonNullable<typeof tree>["roots"][number]): number =>
    1 + node.children.reduce((sum, child) => sum + countNode(child), 0);
  return (tree.anchor ? countNode(tree.anchor) : 0) + tree.roots.reduce((sum, root) => sum + countNode(root), 0);
}

function summarizePinnedMessage(message: AgentMessage): string {
  if (message.toolName) return `Tool: ${message.toolName} (${message.toolStatus ?? "done"})`;
  return "No text content";
}

function formatTokens(value: number): string {
  return Math.max(0, Math.trunc(value)).toLocaleString();
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : "pending";
}
