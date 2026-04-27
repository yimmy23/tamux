import { useMemo, useState, type KeyboardEvent, type UIEvent } from "react";
import { ToolEventRow } from "@/components/agent-chat-panel/chat-view/ToolEventRow";
import { buildDisplayItems } from "@/components/agent-chat-panel/chat-view/helpers";
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
                runtime.openThread(thread.id);
              }}
            >
              <span className="zorai-thread-title">{thread.title}</span>
              {thread.lastMessagePreview && (
                <span className="zorai-thread-preview">{thread.lastMessagePreview}</span>
              )}
              <span className="zorai-thread-meta">
                {threadHistoryLabel(thread)} - {new Date(thread.updatedAt).toLocaleDateString()}
              </span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}

function threadHistoryLabel(thread: AgentThread): string {
  if (thread.messageCount > 0) {
    return `${thread.messageCount} msgs`;
  }
  if ((thread.totalInputTokens ?? 0) > 0 || (thread.totalOutputTokens ?? 0) > 0 || (thread.totalTokens ?? 0) > 0) {
    return "history";
  }
  return "0 msgs";
}

export function ThreadsView() {
  const runtime = useAgentChatPanelRuntime();
  const [pinLimitResult, setPinLimitResult] = useState<AmuxThreadMessagePinResult | null>(null);
  const displayItems = useMemo(() => buildDisplayItems(runtime.messages), [runtime.messages]);

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
  const loadOlderThreadMessages = async (event: UIEvent<HTMLDivElement>) => {
    const scroller = event.currentTarget;
    if (scroller.scrollTop > 24) return;
    const previousHeight = scroller.scrollHeight;
    const loaded = await runtime.loadOlderThreadMessages();
    if (loaded) {
      requestAnimationFrame(() => {
        scroller.scrollTop = scroller.scrollHeight - previousHeight;
      });
    }
  };
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

      <div className="zorai-thread-chat-scroll" onScroll={(event) => void loadOlderThreadMessages(event)}>
        {runtime.messages.length === 0 ? (
          <div className="zorai-thread-empty-state">
            <div className="zorai-brand-mark"><span>Z</span></div>
            <strong>Start a Zorai thread</strong>
            <span>Ask for a plan, delegate work, or turn a request into a goal.</span>
          </div>
        ) : displayItems.map((item) => {
          if (item.type === "tool") {
            return <ToolEventRow key={`tool_${item.group.key}`} group={item.group} />;
          }

          const message = item.message;
          return (
            <MessageBubble
              key={message.id}
              message={message}
              onPin={async () => {
                const result = await runtime.pinMessageForCompaction(runtime.activeThread?.id ?? message.threadId, message.id);
                if (result && result.ok === false && result.error === "pinned_budget_exceeded") {
                  setPinLimitResult(result);
                }
              }}
              onUnpin={() => void runtime.unpinMessageForCompaction(runtime.activeThread?.id ?? message.threadId, message.id)}
            />
          );
        })}
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

      {pinLimitResult ? (
        <PinLimitModal result={pinLimitResult} onClose={() => setPinLimitResult(null)} />
      ) : null}
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

function MessageBubble({
  message,
  onPin,
  onUnpin,
}: {
  message: AgentMessage;
  onPin: () => void | Promise<void>;
  onUnpin: () => void | Promise<void>;
}) {
  const fromUser = message.role === "user";
  const author = message.authorAgentName ?? (fromUser ? "You" : message.role === "assistant" ? "Zorai" : message.role);
  const tokenText = message.totalTokens > 0 ? `${message.totalTokens.toLocaleString()} tokens` : null;

  return (
    <article id={`zorai-message-${message.id}`} className={["zorai-message", fromUser ? "zorai-message--user" : "", message.pinnedForCompaction ? "zorai-message--pinned" : ""].filter(Boolean).join(" ")}>
      <div className="zorai-message__meta">
        <strong>{author}</strong>
        <span>{formatTime(message.createdAt)}{tokenText ? ` / ${tokenText}` : ""}</span>
      </div>
      {message.reasoning ? <p className="zorai-message__reasoning">{message.reasoning}</p> : null}
      <div className="zorai-message__content">{message.content || "No text content"}</div>
      {message.toolCalls && message.toolCalls.length > 0 ? (
        <div className="zorai-message__tools">{message.toolCalls.length} tool calls</div>
      ) : null}
      <div className="zorai-message__actions">
        {message.pinnedForCompaction ? (
          <button type="button" className="zorai-ghost-button" onClick={() => void onUnpin()}>
            Unpin
          </button>
        ) : (
          <button type="button" className="zorai-ghost-button" onClick={() => void onPin()}>
            Pin
          </button>
        )}
      </div>
    </article>
  );
}

function PinLimitModal({
  result,
  onClose,
}: {
  result: AmuxThreadMessagePinResult;
  onClose: () => void;
}) {
  const attempted = Math.max(0, (result.candidate_pinned_chars ?? 0) - result.current_pinned_chars);

  return (
    <div className="zorai-pin-limit-overlay" role="presentation">
      <section className="zorai-pin-limit-dialog" role="dialog" aria-modal="true" aria-labelledby="zorai-pin-limit-title">
        <div className="zorai-section-label">Pin Limit Reached</div>
        <h2 id="zorai-pin-limit-title">This message cannot be pinned for compaction.</h2>
        <p>
          Pinned messages are injected after the owner compaction artifact and are capped
          at 25% of the active model context window.
        </p>
        <div className="zorai-pin-limit-stats">
          <span>Current pinned chars: {result.current_pinned_chars.toLocaleString()}</span>
          <span>Pinned budget chars: {result.pinned_budget_chars.toLocaleString()}</span>
          <span>Attempted total chars: {(result.candidate_pinned_chars ?? 0).toLocaleString()}</span>
          <span>Attempted message size: {attempted.toLocaleString()}</span>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button" onClick={onClose}>Close</button>
        </div>
      </section>
    </div>
  );
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : "pending";
}
