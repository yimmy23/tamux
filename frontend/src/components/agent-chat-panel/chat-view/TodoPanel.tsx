import { useMemo } from "react";
import type { AgentTodoItem } from "../../../lib/agentStore";
import { todoStatusColor } from "./helpers";

export function TodoPanel({
  todos,
  todoPreview,
  expanded,
  onToggle,
}: {
  todos: AgentTodoItem[];
  todoPreview: string;
  expanded: boolean;
  onToggle: () => void;
}) {
  const sortedTodos = useMemo(
    () => todos.slice().sort((a, b) => a.position - b.position),
    [todos],
  );

  if (todos.length === 0) {
    return null;
  }

  return (
    <div
      style={{
        borderTop: "1px solid var(--border)",
        background: "var(--bg-secondary)",
        padding: "var(--space-2) var(--space-3)",
      }}
    >
      <button
        type="button"
        onClick={onToggle}
        style={{
          width: "100%",
          border: "none",
          background: "transparent",
          padding: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "var(--space-2)",
          cursor: "pointer",
          color: "var(--text-primary)",
        }}
      >
        <span style={{ fontSize: "var(--text-xs)", fontWeight: 700 }}>Todo</span>
        <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {todos.length} item{todos.length === 1 ? "" : "s"}{todoPreview ? ` · ${todoPreview}` : ""}
        </span>
      </button>
      {expanded && (
        <div style={{ marginTop: "var(--space-2)", display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
          {sortedTodos.map((item) => (
            <div
              key={item.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
                padding: "6px 8px",
                borderRadius: "var(--radius-sm)",
                background: "var(--bg-tertiary)",
              }}
            >
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: "50%",
                  background: todoStatusColor(item.status),
                  flexShrink: 0,
                }}
              />
              <span style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", flex: 1 }}>
                {item.content}
              </span>
              <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "capitalize" }}>
                {item.status.replace(/_/g, " ")}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
