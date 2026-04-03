import { useState } from "react";
import type { AgentMessage } from "../../../lib/agentStore";
import { parseHandoffSystemEvent } from "./helpers";
import { MarkdownContent } from "./markdown";

function ActionBtn({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      style={{
        background: "transparent",
        border: "none",
        color: "var(--text-muted)",
        cursor: "pointer",
        fontSize: 10,
        fontWeight: 600,
        padding: "3px 6px",
        borderRadius: "var(--radius-sm)",
        transition: "color var(--transition-fast)",
        whiteSpace: "nowrap",
      }}
      onMouseEnter={(event) => {
        event.currentTarget.style.color = "var(--text-primary)";
        event.currentTarget.style.background = "rgba(255,255,255,0.06)";
      }}
      onMouseLeave={(event) => {
        event.currentTarget.style.color = "var(--text-muted)";
        event.currentTarget.style.background = "transparent";
      }}
    >
      {label}
    </button>
  );
}

export function MessageBubble({
  message,
  onCopy,
  onRerun,
  onRegenerate,
  onDelete,
}: {
  message: AgentMessage;
  onCopy?: () => void;
  onRerun?: () => void;
  onRegenerate?: () => void;
  onDelete?: () => void;
}) {
  const isCompactionArtifact = message.messageKind === "compaction_artifact";
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const isTool = message.role === "tool";
  const isAssistant = message.role === "assistant";
  const toolStatusLabel = message.toolStatus ? message.toolStatus.toUpperCase() : "DONE";
  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const [expandedCompaction, setExpandedCompaction] = useState(false);
  const [expandedHandoff, setExpandedHandoff] = useState(false);
  const handoffEvent = isSystem && typeof message.content === "string"
    ? parseHandoffSystemEvent(message.content)
    : null;
  const displayContent = (() => {
    if (!isUser || typeof message.content !== "string") return message.content;
    if (!message.content.startsWith("[Gateway Context]")) return message.content;

    const marker = "User message:\n";
    const markerIndex = message.content.indexOf(marker);
    if (markerIndex < 0) return message.content;

    return message.content.slice(markerIndex + marker.length).trim();
  })();

  const handleCopy = () => {
    onCopy?.();
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (isCompactionArtifact) {
    const visibleContent = expandedCompaction || message.content.length <= 280
      ? message.content
      : `${message.content.slice(0, 280).trimEnd()}...`;

    return (
      <div
        style={{
          width: "100%",
          borderTop: "1px solid color-mix(in srgb, var(--text-muted) 35%, transparent)",
          borderBottom: "1px solid color-mix(in srgb, var(--text-muted) 35%, transparent)",
          padding: "var(--space-3) 0",
          display: "grid",
          gap: "var(--space-2)",
        }}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <div style={{ fontSize: 11, letterSpacing: "0.16em", textTransform: "uppercase", color: "var(--text-muted)" }}>
          ---- auto compaction ----
        </div>
        <div style={{ fontSize: "var(--text-sm)", lineHeight: 1.6, whiteSpace: "pre-wrap", color: "var(--text-secondary)" }}>
          {visibleContent || "rule based"}
        </div>
        {message.content.length > 280 && (
          <button
            onClick={() => setExpandedCompaction((current) => !current)}
            style={{
              background: "transparent",
              border: "1px solid var(--glass-border)",
              color: "var(--text-muted)",
              cursor: "pointer",
              fontSize: 11,
              padding: "4px 8px",
              width: "fit-content",
            }}
          >
            {expandedCompaction ? "Collapse" : "Expand"}
          </button>
        )}
        <div style={{ fontSize: 11, letterSpacing: "0.16em", textTransform: "uppercase", color: "var(--text-muted)" }}>
          ------------------------
        </div>
        {hovered && !message.isStreaming && (
          <div
            style={{
              display: "flex",
              gap: 2,
              background: "var(--bg-secondary)",
              border: "1px solid var(--glass-border)",
              borderRadius: "var(--radius-sm)",
              padding: 2,
              width: "fit-content",
            }}
          >
            <ActionBtn label={copied ? "Copied!" : "Copy"} onClick={handleCopy} />
            {onDelete && <ActionBtn label="Delete" onClick={onDelete} />}
          </div>
        )}
      </div>
    );
  }

  return (
    <div style={{ display: "flex", justifyContent: isUser ? "flex-end" : "flex-start" }}>
      <div
        style={{
          maxWidth: "85%",
          position: "relative",
          borderRadius: "var(--radius-lg)",
          fontSize: "var(--text-sm)",
          lineHeight: 1.6,
          background: isUser ? "var(--bg-secondary)" : isSystem || isTool ? "var(--bg-secondary)" : "transparent",
          color: isUser ? "#b2fff8" : "var(--text-primary)",
          border: "1px solid",
          borderColor: isUser ? "rgba(94, 231, 223, 0.28)" : isSystem || isTool ? "rgba(120, 168, 209, 0.22)" : "transparent",
          wordBreak: "break-word",
          userSelect: "auto",
          fontFamily: "var(--font-mono)",
          padding: isAssistant ? 0 : "var(--space-3)",
        }}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        {isAssistant && (
          <div style={{ color: "#5ee7df", opacity: 0.95, marginBottom: 4, fontSize: 12 }}>{"> assistant"}</div>
        )}

        {isAssistant && message.reasoning && (
          <details style={{ marginTop: 8 }}>
            <summary style={{ cursor: "pointer", fontSize: 11, color: "var(--text-muted)", userSelect: "auto" }}>
              Reasoning
            </summary>
            <div style={{ marginTop: 6, fontSize: 12, color: "var(--text-secondary)", userSelect: "auto" }}>
              <MarkdownContent content={message.reasoning} />
            </div>
          </details>
        )}

        {handoffEvent ? (
          <div style={{ display: "grid", gap: 8 }}>
            <div style={{ fontSize: "var(--text-xs)", color: "var(--agent)", fontWeight: 700 }}>
              Thread Handoff
            </div>
            <div style={{ fontSize: 12, color: "var(--text-primary)" }}>
              {(handoffEvent.from_agent_name ?? "Agent")} {"->"} {(handoffEvent.to_agent_name ?? "Agent")}
            </div>
            {handoffEvent.reason && (
              <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                {handoffEvent.reason}
              </div>
            )}
            {handoffEvent.summary && (
              <div style={{ display: "grid", gap: 6 }}>
                <button
                  onClick={() => setExpandedHandoff((current) => !current)}
                  style={{
                    background: "transparent",
                    border: "1px solid var(--glass-border)",
                    color: "var(--text-muted)",
                    cursor: "pointer",
                    fontSize: 11,
                    padding: "4px 8px",
                    width: "fit-content",
                  }}
                >
                  {expandedHandoff ? "Collapse Summary" : "Expand Summary"}
                </button>
                {expandedHandoff && (
                  <div style={{ padding: "8px", background: "rgba(2, 10, 18, 0.55)", border: "1px solid rgba(120, 168, 209, 0.22)", fontSize: 12, lineHeight: 1.45, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                    {handoffEvent.summary}
                  </div>
                )}
              </div>
            )}
          </div>
        ) : isTool && message.toolName ? (
          <div style={{ display: "grid", gap: 6 }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "center" }}>
              <span style={{ fontSize: "var(--text-xs)", color: "var(--agent)", fontWeight: 700 }}>
                Tool: {message.toolName}
              </span>
              <span style={{ fontSize: 10, color: "var(--text-muted)", border: "1px solid var(--glass-border)", padding: "1px 6px" }}>
                {toolStatusLabel}
              </span>
            </div>

            {message.toolArguments && (
              <pre style={{ margin: 0, padding: "8px", background: "rgba(255,255,255,0.04)", border: "1px solid rgba(255,255,255,0.08)", fontSize: 11, lineHeight: 1.4, whiteSpace: "pre-wrap", wordBreak: "break-word", fontFamily: "var(--font-mono)" }}>
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(message.toolArguments), null, 2);
                  } catch {
                    return message.toolArguments;
                  }
                })()}
              </pre>
            )}

            {message.content && (
              <div style={{ padding: "8px", background: "rgba(2, 10, 18, 0.55)", border: "1px solid rgba(120, 168, 209, 0.22)", fontSize: 12, lineHeight: 1.45, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                {message.content}
              </div>
            )}
          </div>
        ) : (
          <MarkdownContent content={displayContent} />
        )}

        {message.isStreaming && <span style={{ opacity: 0.5, marginLeft: 4 }}>▌</span>}

        {message.model && !isUser && (!isAssistant || hovered) && (
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-1)" }}>
            {message.provider}/{message.model}
          </div>
        )}

        {isAssistant && !message.isStreaming && (message.totalTokens > 0 || message.cost !== undefined || message.tps !== undefined) && (
          <div
            style={{
              fontSize: 11,
              color: "var(--text-muted)",
              marginTop: 4,
              display: "flex",
              flexWrap: "wrap",
              gap: 10,
              opacity: hovered ? 1 : 0,
              maxHeight: hovered ? 40 : 0,
              overflow: "hidden",
              transform: hovered ? "translateY(0)" : "translateY(-4px)",
              transition: "opacity 180ms ease, max-height 240ms ease, transform 180ms ease",
              pointerEvents: hovered ? "auto" : "none",
            }}
          >
            <span>∑ {message.totalTokens.toLocaleString()} (⇅ {message.inputTokens.toLocaleString()} / {message.outputTokens.toLocaleString()})</span>
            {message.reasoningTokens !== undefined && <span>🧠 {message.reasoningTokens}</span>}
            {message.audioTokens !== undefined && message.audioTokens > 0 && <span>🎵 {message.audioTokens}</span>}
            {message.videoTokens !== undefined && message.videoTokens > 0 && <span>🎥 {message.videoTokens}</span>}
            {message.cost !== undefined && <span>${message.cost.toFixed(6)}</span>}
            {message.tps !== undefined && Number.isFinite(message.tps) && <span>↯ {message.tps.toFixed(1)} tok/s</span>}
          </div>
        )}

        {hovered && !message.isStreaming && (
          <div
            style={{
              position: "absolute",
              top: -28,
              right: isUser ? 0 : undefined,
              left: isUser ? undefined : 0,
              display: "flex",
              gap: 2,
              background: "var(--bg-secondary)",
              border: "1px solid var(--glass-border)",
              borderRadius: "var(--radius-sm)",
              padding: 2,
              boxShadow: "var(--shadow-md)",
            }}
          >
            <ActionBtn label={copied ? "Copied!" : "Copy"} onClick={handleCopy} />
            {isUser && onRerun && <ActionBtn label="Rerun" onClick={onRerun} />}
            {isAssistant && onRegenerate && <ActionBtn label="Regen" onClick={onRegenerate} />}
            {onDelete && <ActionBtn label="Delete" onClick={onDelete} />}
          </div>
        )}
      </div>
    </div>
  );
}
