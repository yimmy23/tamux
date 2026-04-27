import { useEffect, useMemo, useState } from "react";
import { SpawnedAgentsPanel } from "@/components/agent-chat-panel/SpawnedAgentsPanel";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentMessage, AgentThread, AgentTodoItem } from "@/lib/agentStore";
import { fetchThreadWorkContext, type ThreadWorkContext, type WorkContextEntry } from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { shortenHomePath } from "@/lib/workspaceStore";
import { workContextKindColor, workContextKindLabel } from "@/components/agent-chat-panel/tasks-view/helpers";

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
        <SpawnedAgentsPanel
          tree={runtime.spawnedAgentTree}
          selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
          canGoBackThread={runtime.canGoBackThread}
          threadNavigationDepth={runtime.threadNavigationDepth}
          backThreadTitle={runtime.backThreadTitle}
          canOpenSpawnedThread={runtime.canOpenSpawnedThread}
          openSpawnedThread={runtime.openSpawnedThread}
          goBackThread={runtime.goBackThread}
          compact
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
  if (entries.length === 0) {
    return <div className="zorai-empty">No file or artifact context recorded for this thread yet.</div>;
  }

  return (
    <section className="zorai-context-list">
      {entries.slice(0, 24).map((entry, index) => (
        <article key={`${entry.source}:${entry.path}:${index}`} className="zorai-context-list-item">
          <div>
            <span style={{ color: workContextKindColor(entry) }}>{workContextKindLabel(entry)}</span>
            <strong>{entry.repoRoot ? `${shortenHomePath(entry.repoRoot)}/${entry.path}` : shortenHomePath(entry.path)}</strong>
            {entry.previousPath ? <span>from {shortenHomePath(entry.previousPath)}</span> : null}
          </div>
        </article>
      ))}
    </section>
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
