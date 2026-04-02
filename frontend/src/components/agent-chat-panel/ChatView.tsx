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
  onStopStreaming,
  onDeleteMessage,
  onUpdateReasoningEffort,
  canStartGoalRun,
  onStartGoalRun,
  welesHealth,
}: ChatViewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [todoExpanded, setTodoExpanded] = useState(true);

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
    </>
  );
}
