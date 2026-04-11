import { useMemo, useState } from "react";
import { buildWelesHealthPresentation } from "./welesHealthPresentation";
import { inputStyle } from "./shared";
import { ChatComposer } from "./chat-view/Composer";
import {
  buildDisplayItems,
  buildTodoPreview,
  filterDisplayItems,
  summarizeSessionUsage,
} from "./chat-view/helpers";
import { MessageBubble } from "./chat-view/MessageBubble";
import { TodoPanel } from "./chat-view/TodoPanel";
import { ToolEventRow } from "./chat-view/ToolEventRow";
import type { ChatViewProps } from "./chat-view/types";

export function ChatView({
  messages,
  todos,
  input,
  setInput,
  inputRef,
  onKeyDown,
  agentSettings,
  isStreamingResponse,
  activeThread,
  messagesEndRef,
  onSendMessage,
  onSendParticipantSuggestion,
  onDismissParticipantSuggestion,
  onStopStreaming,
  onDeleteMessage,
  onUpdateReasoningEffort,
  canStartGoalRun,
  onStartGoalRun,
  welesHealth,
}: ChatViewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [todoExpanded, setTodoExpanded] = useState(true);
  const [participantsModalOpen, setParticipantsModalOpen] = useState(false);

  const handleSendClick = () => {
    const text = input.trim();
    if (!text) return;
    onSendMessage(text);
    setInput("");
  };

  const handleStartGoalRun = async () => {
    const text = input.trim();
    if (!text) return;
    const started = await onStartGoalRun(text);
    if (started) {
      setInput("");
    }
  };

  const displayItems = useMemo(() => buildDisplayItems(messages), [messages]);
  const filteredDisplayItems = useMemo(
    () => filterDisplayItems(displayItems, searchQuery),
    [displayItems, searchQuery],
  );
  const sessionUsageSummary = useMemo(() => summarizeSessionUsage(messages), [messages]);
  const todoPreview = useMemo(() => buildTodoPreview(todos), [todos]);
  const welesHealthPresentation = useMemo(
    () => buildWelesHealthPresentation(welesHealth),
    [welesHealth],
  );
  const activeParticipants = useMemo(
    () => activeThread?.threadParticipants?.filter((participant) => participant.status === "active") ?? [],
    [activeThread],
  );
  const inactiveParticipants = useMemo(
    () => activeThread?.threadParticipants?.filter((participant) => participant.status !== "active") ?? [],
    [activeThread],
  );
  const queuedParticipantSuggestions = useMemo(
    () => activeThread?.queuedParticipantSuggestions ?? [],
    [activeThread],
  );
  const hasParticipantSummary = activeParticipants.length > 0 || inactiveParticipants.length > 0 || queuedParticipantSuggestions.length > 0;

  return (
    <>
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: "8px 8px 8px 16px",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-3)",
        }}
      >
        <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap", alignItems: "center" }}>
          <input
            type="text"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="Search messages and tool output..."
            style={{ ...inputStyle, minWidth: 220 }}
          />
        </div>

        {welesHealthPresentation && (
          <div
            style={{
              border: "1px solid color-mix(in srgb, var(--warning) 55%, var(--border))",
              background: "color-mix(in srgb, var(--warning) 10%, var(--bg-secondary))",
              borderRadius: "var(--radius-lg)",
              padding: "var(--space-3)",
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-1)",
            }}
          >
            <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--warning)" }}>
              {welesHealthPresentation.title}
            </div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
              {welesHealthPresentation.detail}
            </div>
          </div>
        )}

        {activeThread && hasParticipantSummary && (
          <div
            style={{
              border: "1px solid var(--border)",
              background: "var(--bg-secondary)",
              borderRadius: "var(--radius-lg)",
              padding: "var(--space-3)",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "var(--space-3)",
              flexWrap: "wrap",
            }}
          >
            <div style={{ display: "grid", gap: "var(--space-1)" }}>
              <div style={{ fontSize: "var(--text-xs)", fontWeight: 700, color: "var(--text-muted)", letterSpacing: "0.08em", textTransform: "uppercase" }}>
                Thread Participants
              </div>
              <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                <span>{activeParticipants.length} active</span>
                <span>{inactiveParticipants.length} inactive</span>
                <span>{queuedParticipantSuggestions.length} queued</span>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setParticipantsModalOpen(true)}
              style={{ border: "1px solid var(--accent)", background: "rgba(94, 231, 223, 0.16)", color: "var(--accent)", borderRadius: "var(--radius-sm)", padding: "6px 12px", fontSize: 12, fontWeight: 700, cursor: "pointer" }}
            >
              View Details
            </button>
          </div>
        )}

        {filteredDisplayItems.length === 0 && (
          <div className="amux-empty-state">
            <div className="amux-empty-state__icon">✨</div>
            <div className="amux-empty-state__title">
              {messages.length === 0 ? "Start a conversation" : "No chat items match filters"}
            </div>
            <div className="amux-empty-state__description">
              {messages.length === 0 ? "Send a message to begin collaborating with the agent" : "Try a different search term."}
            </div>
          </div>
        )}

        {filteredDisplayItems.map((item) => {
          if (item.type === "tool") {
            return <ToolEventRow key={`tool_${item.group.key}`} group={item.group} />;
          }

          const message = item.message;
          return (
            <MessageBubble
              key={message.id}
              message={message}
              onCopy={() => {
                try {
                  navigator.clipboard.writeText(message.content);
                } catch {
                  // Ignore clipboard failures.
                }
              }}
              onRerun={message.role === "user" ? () => onSendMessage(message.content) : undefined}
              onRegenerate={message.role === "assistant" ? () => {
                const idx = messages.findIndex((entry) => entry.id === message.id);
                if (idx <= 0) {
                  return;
                }
                const prevUserMsg = messages.slice(0, idx).reverse().find((entry) => entry.role === "user");
                if (prevUserMsg) {
                  onSendMessage(prevUserMsg.content);
                }
              } : undefined}
              onDelete={onDeleteMessage ? () => onDeleteMessage(message.id) : undefined}
            />
          );
        })}
        <div ref={messagesEndRef} />
      </div>

      {activeThread && activeThread.totalTokens > 0 && (
        <div
          style={{
            padding: "var(--space-2) var(--space-3)",
            fontSize: "var(--text-xs)",
            color: "var(--text-muted)",
            borderTop: "1px solid var(--border)",
            display: "flex",
            gap: "var(--space-3)",
          }}
        >
          <span>In: {activeThread.totalInputTokens.toLocaleString()}</span>
          <span>Out: {activeThread.totalOutputTokens.toLocaleString()}</span>
          <span>Total: {activeThread.totalTokens.toLocaleString()}</span>
          {sessionUsageSummary.hasCost && (
            <span>Cost: ${sessionUsageSummary.totalCost.toFixed(6)}</span>
          )}
          {typeof sessionUsageSummary.avgTps === "number" && (
            <span>Avg TPS: {sessionUsageSummary.avgTps.toFixed(1)} tok/s</span>
          )}
          {activeThread.compactionCount > 0 && (
            <span>Compacted: {activeThread.compactionCount}×</span>
          )}
        </div>
      )}

      <TodoPanel
        todos={todos}
        todoPreview={todoPreview}
        expanded={todoExpanded}
        onToggle={() => setTodoExpanded((current) => !current)}
      />

      <ChatComposer
        input={input}
        setInput={setInput}
        inputRef={inputRef}
        onKeyDown={onKeyDown}
        agentSettings={agentSettings}
        isStreamingResponse={isStreamingResponse}
        onStopStreaming={onStopStreaming}
        onSend={handleSendClick}
        canStartGoalRun={canStartGoalRun}
        onStartGoalRun={() => {
          void handleStartGoalRun();
        }}
        onUpdateReasoningEffort={onUpdateReasoningEffort}
      />

      {participantsModalOpen && activeThread && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(3, 8, 18, 0.7)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "var(--space-6)",
            zIndex: 1000,
          }}
        >
          <div
            style={{
              width: "min(760px, 100%)",
              maxHeight: "80vh",
              overflow: "auto",
              border: "1px solid var(--border)",
              background: "var(--bg-primary)",
              borderRadius: "var(--radius-xl)",
              padding: "var(--space-4)",
              display: "grid",
              gap: "var(--space-4)",
              boxShadow: "0 24px 80px rgba(0, 0, 0, 0.45)",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-3)" }}>
              <div>
                <div style={{ fontSize: "var(--text-xs)", fontWeight: 700, color: "var(--text-muted)", letterSpacing: "0.08em", textTransform: "uppercase" }}>
                  Thread Participants
                </div>
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{activeThread.title}</div>
              </div>
              <button
                type="button"
                onClick={() => setParticipantsModalOpen(false)}
                style={{ border: "1px solid var(--glass-border)", background: "transparent", color: "var(--text-muted)", borderRadius: "var(--radius-sm)", padding: "6px 10px", fontSize: 12, cursor: "pointer" }}
              >
                Close
              </button>
            </div>

            <div style={{ display: "grid", gap: "var(--space-3)" }}>
              <div style={{ display: "grid", gap: "var(--space-2)" }}>
                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Active Participants</div>
                {activeParticipants.length === 0 ? (
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)" }}>None</div>
                ) : activeParticipants.map((participant) => (
                  <div key={`${participant.agentId}:active`} style={{ border: "1px solid var(--glass-border)", borderRadius: "var(--radius-md)", padding: "var(--space-3)", display: "grid", gap: "var(--space-1)", background: "var(--bg-secondary)" }}>
                    <div style={{ fontWeight: 700 }}>{participant.agentName}</div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>{participant.instruction}</div>
                  </div>
                ))}
              </div>

              <div style={{ display: "grid", gap: "var(--space-2)" }}>
                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Inactive Participants</div>
                {inactiveParticipants.length === 0 ? (
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)" }}>None</div>
                ) : inactiveParticipants.map((participant) => (
                  <div key={`${participant.agentId}:inactive`} style={{ border: "1px solid var(--glass-border)", borderRadius: "var(--radius-md)", padding: "var(--space-3)", display: "grid", gap: "var(--space-1)", background: "var(--bg-secondary)" }}>
                    <div style={{ fontWeight: 700 }}>{participant.agentName}</div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>{participant.instruction}</div>
                  </div>
                ))}
              </div>

              <div style={{ display: "grid", gap: "var(--space-2)" }}>
                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Queued Suggestions</div>
                {queuedParticipantSuggestions.length === 0 ? (
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)" }}>None</div>
                ) : queuedParticipantSuggestions.map((suggestion) => (
                  <div
                    key={suggestion.id}
                    style={{
                      display: "grid",
                      gap: "var(--space-2)",
                      border: "1px solid var(--glass-border)",
                      borderRadius: "var(--radius-md)",
                      padding: "var(--space-3)",
                      background: "var(--bg-secondary)",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-2)", alignItems: "center", flexWrap: "wrap" }}>
                      <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center", flexWrap: "wrap" }}>
                        <span style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>{suggestion.targetAgentName}</span>
                        {suggestion.forceSend && (
                          <span style={{ fontSize: 11, border: "1px solid var(--warning)", color: "var(--warning)", borderRadius: 999, padding: "2px 8px" }}>
                            Force Send
                          </span>
                        )}
                        {suggestion.status === "failed" && (
                          <span style={{ fontSize: 11, border: "1px solid #ff7675", color: "#ff7675", borderRadius: 999, padding: "2px 8px" }}>
                            Failed
                          </span>
                        )}
                      </div>
                      <div style={{ display: "flex", gap: "var(--space-2)" }}>
                        <button
                          type="button"
                          onClick={() => { void onSendParticipantSuggestion(activeThread.daemonThreadId ?? activeThread.id, suggestion.id, suggestion.forceSend); }}
                          style={{ border: "1px solid var(--accent)", background: "rgba(94, 231, 223, 0.16)", color: "var(--accent)", borderRadius: "var(--radius-sm)", padding: "6px 10px", fontSize: 12, fontWeight: 700, cursor: "pointer" }}
                        >
                          Send Now
                        </button>
                        <button
                          type="button"
                          onClick={() => { void onDismissParticipantSuggestion(activeThread.daemonThreadId ?? activeThread.id, suggestion.id); }}
                          style={{ border: "1px solid var(--glass-border)", background: "transparent", color: "var(--text-muted)", borderRadius: "var(--radius-sm)", padding: "6px 10px", fontSize: 12, cursor: "pointer" }}
                        >
                          Dismiss
                        </button>
                      </div>
                    </div>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>{suggestion.instruction}</div>
                    {suggestion.error && <div style={{ fontSize: 12, color: "#ff7675" }}>{suggestion.error}</div>}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
