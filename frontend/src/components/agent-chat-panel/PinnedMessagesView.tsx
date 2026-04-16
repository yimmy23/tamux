import type { AgentMessage } from "@/lib/agentStore";

export function PinnedMessagesView({
  messages,
  pinnedUsageChars,
  pinnedBudgetChars,
  pinnedOverBudget,
  onJumpToMessage,
  onUnpinMessage,
}: {
  messages: AgentMessage[];
  pinnedUsageChars: number;
  pinnedBudgetChars: number;
  pinnedOverBudget: boolean;
  onJumpToMessage: (messageId: string) => void;
  onUnpinMessage: (messageId: string) => void | Promise<unknown>;
}) {
  return (
    <div style={{ flex: 1, overflow: "auto", padding: "var(--space-4)", display: "grid", gap: "var(--space-3)" }}>
      <div
        style={{
          border: "1px solid var(--border)",
          background: "var(--bg-secondary)",
          borderRadius: "var(--radius-lg)",
          padding: "var(--space-3)",
          display: "grid",
          gap: "var(--space-2)",
        }}
      >
        <div style={{ fontSize: "var(--text-xs)", fontWeight: 700, color: "var(--text-muted)", letterSpacing: "0.08em", textTransform: "uppercase" }}>
          Pinned Compaction Context
        </div>
        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>
          {pinnedUsageChars.toLocaleString()} / {pinnedBudgetChars.toLocaleString()} chars
        </div>
        {pinnedOverBudget && (
          <div style={{ fontSize: "var(--text-sm)", color: "var(--warning)", lineHeight: 1.5 }}>
            Some pinned messages are currently excluded during compaction because the thread is over the active model budget.
          </div>
        )}
      </div>

      {messages.map((message) => (
        <div
          key={message.id}
          style={{
            border: "1px solid var(--border)",
            background: "var(--bg-secondary)",
            borderRadius: "var(--radius-lg)",
            padding: "var(--space-3)",
            display: "grid",
            gap: "var(--space-2)",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-3)", alignItems: "center" }}>
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", letterSpacing: "0.08em", textTransform: "uppercase" }}>
              {message.role} · {message.content.length.toLocaleString()} chars
            </div>
            <div style={{ display: "flex", gap: "var(--space-2)" }}>
              <button
                type="button"
                onClick={() => onJumpToMessage(message.id)}
                style={{ border: "1px solid var(--glass-border)", background: "transparent", color: "var(--text-primary)", borderRadius: "var(--radius-sm)", padding: "6px 10px", fontSize: 12, cursor: "pointer" }}
              >
                Jump to message
              </button>
              <button
                type="button"
                onClick={() => { void onUnpinMessage(message.id); }}
                style={{ border: "1px solid color-mix(in srgb, var(--warning) 50%, var(--border))", background: "transparent", color: "var(--warning)", borderRadius: "var(--radius-sm)", padding: "6px 10px", fontSize: 12, cursor: "pointer" }}
              >
                Unpin
              </button>
            </div>
          </div>
          <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.6, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
            {message.content}
          </div>
        </div>
      ))}
    </div>
  );
}
