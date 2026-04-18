import { useEffect, useMemo, useRef, useState } from "react";
import { buildWelesHealthPresentation } from "./welesHealthPresentation";
import { inputStyle } from "./shared";
import { ChatComposer } from "./chat-view/Composer";
import {
  buildDisplayItems,
  buildTodoPreview,
  filterDisplayItems,
  summarizeSessionUsage,
} from "./chat-view/helpers";
import { compactionArtifactDisplayText, MessageBubble } from "./chat-view/MessageBubble";
import { TodoPanel } from "./chat-view/TodoPanel";
import { ToolEventRow } from "./chat-view/ToolEventRow";
import type { AgentMessage } from "@/lib/agentStore";
import type { ChatViewProps, ComposerAttachment, SendMessagePayload } from "./chat-view/types";

function buildAttachmentSendPayload(text: string, attachments: ComposerAttachment[]): SendMessagePayload {
  const trimmedText = text.trim();
  const textAttachmentWrappers = attachments
    .filter((attachment) => attachment.kind === "text" && attachment.textContent)
    .map((attachment) => `<attached_file name="${attachment.name}">\n${attachment.textContent}\n</attached_file>`);
  const mediaAttachments = attachments.filter((attachment) => attachment.kind !== "text");
  const finalText = [...textAttachmentWrappers, trimmedText].filter(Boolean).join("\n\n").trim();

  if (mediaAttachments.length === 0) {
    return { text: finalText };
  }
  const localContentBlocks = [
    ...(finalText ? [{ type: "text", text: finalText } as const] : []),
    ...mediaAttachments.map((attachment) =>
      attachment.kind === "image"
        ? ({
            type: "image",
            data_url: attachment.dataUrl,
            mime_type: attachment.mimeType,
          } as const)
        : ({
            type: "audio",
            data_url: attachment.dataUrl,
            mime_type: attachment.mimeType,
          } as const),
    ),
  ];
  return {
    text: finalText,
    contentBlocksJson: JSON.stringify(localContentBlocks),
    localContentBlocks,
  };
}

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
  onPinMessage,
  onUnpinMessage,
  onUpdateReasoningEffort,
  canStartGoalRun,
  onStartGoalRun,
  welesHealth,
}: ChatViewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [todoExpanded, setTodoExpanded] = useState(true);
  const [participantsModalOpen, setParticipantsModalOpen] = useState(false);
  const [pinLimitResult, setPinLimitResult] = useState<AmuxThreadMessagePinResult | null>(null);
  const [composerAttachments, setComposerAttachments] = useState<ComposerAttachment[]>([]);
  const [autoSpeakReplies, setAutoSpeakReplies] = useState(agentSettings.audio_tts_auto_speak);
  const [speakingMessageId, setSpeakingMessageId] = useState<string | null>(null);
  const activeAudioRef = useRef<HTMLAudioElement | null>(null);
  const lastAutoSpokenMessageIdRef = useRef<string | null>(null);

  const handleSendClick = () => {
    const text = input.trim();
    if (!text && composerAttachments.length === 0) return;
    onSendMessage(buildAttachmentSendPayload(text, composerAttachments));
    setInput("");
    setComposerAttachments([]);
  };

  const handleStartGoalRun = async () => {
    const text = input.trim();
    if (!text) return;
    const started = await onStartGoalRun(text);
    if (started) {
      setInput("");
    }
  };

  const stopAudioPlayback = () => {
    if (activeAudioRef.current) {
      activeAudioRef.current.pause();
      activeAudioRef.current.currentTime = 0;
      activeAudioRef.current = null;
    }
    setSpeakingMessageId(null);
  };

  const speakMessage = async (message: AgentMessage) => {
    const bridge = window.amux ?? window.tamux;
    if (!bridge?.agentTextToSpeech || !agentSettings.audio_tts_enabled) {
      return;
    }

    const messageId = "id" in message ? message.id : null;
    if (messageId && speakingMessageId === messageId) {
      stopAudioPlayback();
      return;
    }

    const text = "content" in message ? compactionArtifactDisplayText(message).trim() : "";
    if (!text) {
      return;
    }

    stopAudioPlayback();
    if (messageId) {
      setSpeakingMessageId(messageId);
    }
    try {
      const result = await bridge.agentTextToSpeech(text, agentSettings.audio_tts_voice || null, {
        provider: agentSettings.audio_tts_provider,
        model: agentSettings.audio_tts_model,
      });
      const source = typeof result === "string"
        ? result
        : result && typeof result === "object"
          ? (typeof (result as { file_url?: unknown }).file_url === "string"
              ? (result as { file_url: string }).file_url
              : typeof (result as { url?: unknown }).url === "string"
                ? (result as { url: string }).url
                : typeof (result as { path?: unknown }).path === "string"
                  ? (result as { path: string }).path
                  : "")
          : "";
      if (!source) {
        stopAudioPlayback();
        return;
      }
      const audio = new Audio(source);
      activeAudioRef.current = audio;
      audio.onended = () => {
        if (activeAudioRef.current === audio) {
          activeAudioRef.current = null;
          setSpeakingMessageId(null);
        }
      };
      audio.onerror = () => {
        if (activeAudioRef.current === audio) {
          activeAudioRef.current = null;
          setSpeakingMessageId(null);
        }
      };
      await audio.play();
    } catch (error) {
      console.error("text-to-speech failed", error);
      stopAudioPlayback();
    }
  };

  useEffect(() => {
    setAutoSpeakReplies(agentSettings.audio_tts_auto_speak);
  }, [agentSettings.audio_tts_auto_speak]);

  useEffect(() => {
    return () => {
      stopAudioPlayback();
    };
  }, []);

  useEffect(() => {
    if (!autoSpeakReplies || messages.length === 0) {
      return;
    }
    const latestAssistantMessage = [...messages]
      .reverse()
      .find((message) => message.role === "assistant" && !message.isStreaming && compactionArtifactDisplayText(message).trim());
    if (!latestAssistantMessage) {
      return;
    }
    if (lastAutoSpokenMessageIdRef.current === latestAssistantMessage.id) {
      return;
    }
    lastAutoSpokenMessageIdRef.current = latestAssistantMessage.id;
    void speakMessage(latestAssistantMessage);
  }, [autoSpeakReplies, messages]);

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
                  navigator.clipboard.writeText(compactionArtifactDisplayText(message));
                } catch {
                  // Ignore clipboard failures.
                }
              }}
              onRerun={message.role === "user" ? () => onSendMessage({ text: message.content }) : undefined}
              onRegenerate={message.role === "assistant" ? () => {
                const idx = messages.findIndex((entry) => entry.id === message.id);
                if (idx <= 0) {
                  return;
                }
                const prevUserMsg = messages.slice(0, idx).reverse().find((entry) => entry.role === "user");
                if (prevUserMsg) {
                  onSendMessage({ text: prevUserMsg.content });
                }
              } : undefined}
              onDelete={onDeleteMessage ? () => onDeleteMessage(message.id) : undefined}
              onPin={onPinMessage ? async () => {
                const result = await onPinMessage(message.id);
                if (result && result.ok === false && result.error === "pinned_budget_exceeded") {
                  setPinLimitResult(result);
                }
              } : undefined}
              onUnpin={onUnpinMessage ? async () => {
                await onUnpinMessage(message.id);
              } : undefined}
              onSpeak={message.role === "assistant" ? async () => {
                await speakMessage(message);
              } : undefined}
              isSpeaking={speakingMessageId === message.id}
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
        attachments={composerAttachments}
        setAttachments={setComposerAttachments}
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

      <div style={{ padding: "0 var(--space-3) var(--space-2)", display: "flex", justifyContent: "flex-end" }}>
        <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11, color: "var(--text-muted)", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={autoSpeakReplies}
            onChange={(event) => {
              setAutoSpeakReplies(event.target.checked);
              if (!event.target.checked) {
                stopAudioPlayback();
              }
            }}
          />
          Auto-speak replies
        </label>
      </div>

      {pinLimitResult && (
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
              width: "min(520px, 100%)",
              border: "1px solid color-mix(in srgb, var(--warning) 55%, var(--border))",
              background: "var(--bg-primary)",
              borderRadius: "var(--radius-xl)",
              padding: "var(--space-4)",
              display: "grid",
              gap: "var(--space-3)",
              boxShadow: "0 24px 80px rgba(0, 0, 0, 0.45)",
            }}
          >
            <div style={{ fontSize: "var(--text-xs)", fontWeight: 700, color: "var(--warning)", letterSpacing: "0.08em", textTransform: "uppercase" }}>
              Pin Limit Reached
            </div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.6 }}>
              This message cannot be pinned because pinned messages are sent as separate compaction messages and are capped at 25% of the active model context window.
            </div>
            <div style={{ display: "grid", gap: "var(--space-1)", fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>
              <div>Current pinned usage: {pinLimitResult.current_pinned_chars.toLocaleString()} chars</div>
              <div>Pin budget: {pinLimitResult.pinned_budget_chars.toLocaleString()} chars</div>
              <div>Candidate total: {(pinLimitResult.candidate_pinned_chars ?? 0).toLocaleString()} chars</div>
              <div>Attempted message size: {Math.max(0, (pinLimitResult.candidate_pinned_chars ?? 0) - pinLimitResult.current_pinned_chars).toLocaleString()} chars</div>
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end" }}>
              <button
                type="button"
                onClick={() => setPinLimitResult(null)}
                style={{ border: "1px solid var(--glass-border)", background: "transparent", color: "var(--text-primary)", borderRadius: "var(--radius-sm)", padding: "6px 12px", fontSize: 12, cursor: "pointer" }}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

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
