import type React from "react";
import { inputStyle } from "../shared";

export function ChatComposer({
  input,
  setInput,
  inputRef,
  onKeyDown,
  agentSettings,
  isStreamingResponse,
  onStopStreaming,
  onSend,
  canStartGoalRun,
  onStartGoalRun,
  onUpdateReasoningEffort,
}: {
  input: string;
  setInput: (value: string) => void;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (event: React.KeyboardEvent) => void;
  agentSettings: { enabled: boolean; chatFontFamily: string; reasoning_effort: string };
  isStreamingResponse: boolean;
  onStopStreaming: () => void;
  onSend: () => void;
  canStartGoalRun: boolean;
  onStartGoalRun: () => void;
  onUpdateReasoningEffort: (value: string) => void;
}) {
  return (
    <div
      style={{
        padding: "var(--space-3)",
        borderTop: "1px solid var(--border)",
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        background: "var(--bg-tertiary)",
        userSelect: "auto",
      }}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "auto 1fr",
          alignItems: "start",
          gap: "var(--space-2)",
          border: "1px solid rgba(94, 231, 223, 0.3)",
          background: "var(--bg-tertiary)",
          borderRadius: "var(--radius-md)",
          padding: "8px 10px",
        }}
      >
        <span
          style={{
            color: "#5ee7df",
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-sm)",
            lineHeight: "24px",
            userSelect: "auto",
          }}
        >
          &gt;
        </span>
        <textarea
          ref={inputRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={onKeyDown}
          rows={3}
          placeholder={agentSettings.enabled ? "Type a message... (Enter to send, Ctrl+Enter for newline)" : "Agent disabled — enable in Settings > Agent"}
          disabled={!agentSettings.enabled}
          style={{
            ...inputStyle,
            width: "100%",
            resize: "none",
            background: "transparent",
            border: "none",
            color: "var(--text-primary)",
            padding: "4px 0",
            fontFamily: agentSettings.chatFontFamily,
            outline: "none",
            opacity: agentSettings.enabled ? 1 : 0.5,
            minHeight: 72,
          }}
        />
      </div>

      <div style={{ marginTop: "var(--space-2)", display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-2)" }}>
        <div style={{ display: "flex", alignItems: "flex-start", flexDirection: "column", gap: 4 }}>
          <span style={{ fontSize: 11, color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
            Reasoning effort
          </span>
          <select
            value={agentSettings.reasoning_effort}
            onChange={(event) => onUpdateReasoningEffort(event.target.value)}
            title="Reasoning effort"
            style={{
              fontSize: 10,
              fontFamily: "var(--font-mono)",
              background: "var(--bg-surface)",
              color: "var(--text-secondary)",
              border: "1px solid var(--glass-border)",
              borderRadius: 3,
              padding: "1px 4px",
              cursor: "pointer",
              outline: "none",
            }}
          >
            <option value="none">off</option>
            <option value="minimal">minimal</option>
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
            <option value="xhigh">xhigh</option>
          </select>
        </div>
        <div style={{ display: "flex", gap: "var(--space-2)" }}>
          {canStartGoalRun && (
            <button
              type="button"
              onClick={onStartGoalRun}
              disabled={!agentSettings.enabled || !input.trim()}
              style={{
                border: "1px solid var(--mission-border)",
                background: "var(--mission-soft)",
                color: "var(--mission)",
                borderRadius: "var(--radius-sm)",
                padding: "6px 12px",
                fontSize: 12,
                fontWeight: 700,
                cursor: !agentSettings.enabled || !input.trim() ? "not-allowed" : "pointer",
                opacity: !agentSettings.enabled || !input.trim() ? 0.5 : 1,
              }}
            >
              Goal Run
            </button>
          )}
          {isStreamingResponse && (
            <button
              type="button"
              onClick={onStopStreaming}
              style={{
                border: "1px solid rgba(255, 118, 117, 0.45)",
                background: "rgba(255, 118, 117, 0.15)",
                color: "#ff7675",
                borderRadius: "var(--radius-sm)",
                padding: "6px 10px",
                fontSize: 12,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              Stop
            </button>
          )}
          <button
            type="button"
            onClick={onSend}
            disabled={!agentSettings.enabled || !input.trim()}
            style={{
              border: "1px solid var(--accent)",
              background: "rgba(94, 231, 223, 0.16)",
              color: "var(--accent)",
              borderRadius: "var(--radius-sm)",
              padding: "6px 12px",
              fontSize: 12,
              fontWeight: 700,
              cursor: !agentSettings.enabled || !input.trim() ? "not-allowed" : "pointer",
              opacity: !agentSettings.enabled || !input.trim() ? 0.5 : 1,
            }}
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
