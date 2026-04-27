import type { KeyboardEvent } from "react";
import { SpawnedAgentsPanel } from "@/components/agent-chat-panel/SpawnedAgentsPanel";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";

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

  const sendCurrentInput = () => runtime.handleSend();
  const startGoal = () => {
    const prompt = runtime.input.trim();
    if (!prompt) return;
    void runtime.startGoalRunFromPrompt(prompt).then((started) => {
      if (started) runtime.setInput("");
    });
  };

  return (
    <section className="zorai-thread-surface zorai-native-thread-surface">
      <ThreadHeader
        thread={runtime.activeThread}
        messageCount={runtime.messages.length}
        streaming={runtime.isStreamingResponse}
        onStop={() => runtime.stopStreaming(runtime.activeThreadId)}
      />
      <ParticipantStrip thread={runtime.activeThread} />

      <div className="zorai-thread-chat-scroll">
        {runtime.messages.length === 0 ? (
          <div className="zorai-thread-empty-state">
            <div className="zorai-brand-mark"><span>Z</span></div>
            <strong>Start a Zorai thread</strong>
            <span>Ask for a plan, delegate work, or turn a request into a goal.</span>
          </div>
        ) : (
          runtime.messages.map((message) => <MessageBubble key={message.id} message={message} />)
        )}
        <div ref={runtime.messagesEndRef} />
      </div>

      <div className="zorai-thread-composer">
        <textarea
          ref={runtime.inputRef}
          value={runtime.input}
          onChange={(event) => runtime.setInput(event.target.value)}
          onKeyDown={(event: KeyboardEvent<HTMLTextAreaElement>) => runtime.handleKeyDown(event)}
          placeholder="Message Zorai..."
          rows={3}
        />
        <div className="zorai-thread-composer__footer">
          <span>Enter sends. Shift+Enter adds a new line.</span>
          <div className="zorai-card-actions">
            {runtime.canStartGoalRun ? (
              <button type="button" className="zorai-ghost-button" onClick={startGoal} disabled={!runtime.input.trim()}>
                Start Goal
              </button>
            ) : null}
            <button
              type="button"
              className="zorai-primary-button"
              onClick={sendCurrentInput}
              disabled={!runtime.input.trim() || runtime.isStreamingResponse}
            >
              Send
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}

function ThreadHeader({
  thread,
  messageCount,
  streaming,
  onStop,
}: {
  thread: AgentThread;
  messageCount: number;
  streaming: boolean;
  onStop: () => void;
}) {
  return (
    <header className="zorai-thread-header">
      <div>
        <div className="zorai-kicker">Thread</div>
        <h2>{thread.title}</h2>
        <span>{messageCount} messages / {thread.agent_name}</span>
      </div>
      {streaming ? (
        <button type="button" className="zorai-ghost-button" onClick={onStop}>
          Stop
        </button>
      ) : null}
    </header>
  );
}

function ParticipantStrip({ thread }: { thread: AgentThread }) {
  const participants = thread.threadParticipants ?? [];
  const queued = thread.queuedParticipantSuggestions ?? [];
  if (participants.length === 0 && queued.length === 0) return null;

  return (
    <div className="zorai-thread-participants">
      {participants.map((participant) => (
        <span key={participant.agentId} className="zorai-status-pill">
          {participant.agentName} / {participant.status}
        </span>
      ))}
      {queued.map((suggestion) => (
        <span key={suggestion.id} className="zorai-status-pill">
          queued: {suggestion.targetAgentName}
        </span>
      ))}
    </div>
  );
}

function MessageBubble({ message }: { message: AgentMessage }) {
  const fromUser = message.role === "user";
  const author = message.authorAgentName ?? (fromUser ? "You" : message.role === "assistant" ? "Zorai" : message.role);
  const tokenText = message.totalTokens > 0 ? `${message.totalTokens.toLocaleString()} tokens` : null;

  return (
    <article className={["zorai-message", fromUser ? "zorai-message--user" : ""].filter(Boolean).join(" ")}>
      <div className="zorai-message__meta">
        <strong>{author}</strong>
        <span>{formatTime(message.createdAt)}{tokenText ? ` / ${tokenText}` : ""}</span>
      </div>
      {message.reasoning ? <p className="zorai-message__reasoning">{message.reasoning}</p> : null}
      <div className="zorai-message__content">{message.content || summarizeToolMessage(message)}</div>
      {message.toolCalls && message.toolCalls.length > 0 ? (
        <div className="zorai-message__tools">{message.toolCalls.length} tool calls</div>
      ) : null}
    </article>
  );
}

function summarizeToolMessage(message: AgentMessage): string {
  if (message.toolName) return `${message.toolName}: ${message.toolStatus ?? "done"}`;
  return "No text content";
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : "pending";
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
