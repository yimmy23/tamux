import { ChatView } from "@/components/agent-chat-panel/ChatView";
import { SpawnedAgentsPanel } from "@/components/agent-chat-panel/SpawnedAgentsPanel";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentSettings } from "@/lib/agentStore";

export function ThreadsRail() {
  const runtime = useAgentChatPanelRuntime();

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-rail-actions">
        <button
          type="button"
          className="zorai-primary-button"
          onClick={() => {
            runtime.createThread({ workspaceId: runtime.activeWorkspace?.id ?? null });
            runtime.setChatBackView("threads");
            runtime.setView("chat");
          }}
        >
          New Thread
        </button>
        <button type="button" className="zorai-ghost-button" onClick={() => void runtime.refreshThreadList()}>
          Refresh
        </button>
      </div>
      <input
        className="zorai-search-input"
        value={runtime.searchQuery}
        onChange={(event) => runtime.setSearchQuery(event.target.value)}
        placeholder="Search threads"
      />
      <div className="zorai-thread-list">
        {runtime.filteredThreads.length === 0 ? (
          <div className="zorai-empty">No threads match this search.</div>
        ) : (
          runtime.filteredThreads.map((thread) => (
            <button
              type="button"
              key={thread.id}
              className={[
                "zorai-thread-item",
                thread.id === runtime.activeThreadId ? "zorai-thread-item--active" : "",
              ].filter(Boolean).join(" ")}
              onClick={() => {
                runtime.setActiveThread(thread.id);
                runtime.setChatBackView("threads");
                runtime.setView("chat");
              }}
            >
              <span className="zorai-thread-title">{thread.title}</span>
              {thread.lastMessagePreview && (
                <span className="zorai-thread-preview">{thread.lastMessagePreview}</span>
              )}
              <span className="zorai-thread-meta">
                {thread.messageCount} msgs - {new Date(thread.updatedAt).toLocaleDateString()}
              </span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}

export function ThreadsView() {
  const runtime = useAgentChatPanelRuntime();

  if (!runtime.activeThread) {
    return (
      <div className="zorai-empty-main">
        <div className="zorai-empty-kicker">Zorai</div>
        <h1>Start with a thread.</h1>
        <p>
          Threads are the default Zorai surface. Create a conversation, bring in an agent,
          then promote durable work into goals or workspace cards when needed.
        </p>
        <button
          type="button"
          className="zorai-primary-button"
          onClick={() => runtime.createThread({ workspaceId: runtime.activeWorkspace?.id ?? null })}
        >
          New Thread
        </button>
      </div>
    );
  }

  return (
    <section className="zorai-thread-surface">
      <ChatView
        messages={runtime.messages}
        todos={runtime.todos}
        input={runtime.input}
        setInput={runtime.setInput}
        inputRef={runtime.inputRef}
        onKeyDown={runtime.handleKeyDown}
        agentSettings={runtime.agentSettings}
        isStreamingResponse={runtime.isStreamingResponse}
        activeThread={runtime.activeThread}
        messagesEndRef={runtime.messagesEndRef}
        onSendMessage={runtime.sendMessage}
        onSendParticipantSuggestion={runtime.sendParticipantSuggestion}
        onDismissParticipantSuggestion={runtime.dismissParticipantSuggestion}
        onStopStreaming={() => runtime.stopStreaming(runtime.activeThreadId)}
        onDeleteMessage={(messageId) => {
          if (runtime.activeThreadId) runtime.deleteMessage(runtime.activeThreadId, messageId);
        }}
        onPinMessage={
          runtime.activeThreadId
            ? (messageId) => runtime.pinMessageForCompaction(runtime.activeThreadId as string, messageId)
            : undefined
        }
        onUnpinMessage={
          runtime.activeThreadId
            ? (messageId) => runtime.unpinMessageForCompaction(runtime.activeThreadId as string, messageId)
            : undefined
        }
        onUpdateReasoningEffort={(value) =>
          runtime.updateAgentSetting("reasoning_effort", value as AgentSettings["reasoning_effort"])
        }
        canStartGoalRun={runtime.canStartGoalRun}
        onStartGoalRun={runtime.startGoalRunFromPrompt}
        welesHealth={runtime.welesHealth}
      />
    </section>
  );
}

export function ThreadsContext() {
  const runtime = useAgentChatPanelRuntime();

  return (
    <SpawnedAgentsPanel
      tree={runtime.spawnedAgentTree}
      selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
      canGoBackThread={runtime.canGoBackThread}
      threadNavigationDepth={runtime.threadNavigationDepth}
      backThreadTitle={runtime.backThreadTitle}
      canOpenSpawnedThread={runtime.canOpenSpawnedThread}
      openSpawnedThread={runtime.openSpawnedThread}
      goBackThread={runtime.goBackThread}
    />
  );
}
