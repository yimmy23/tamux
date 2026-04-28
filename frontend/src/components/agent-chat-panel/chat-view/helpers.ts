import type { AgentMessage, AgentTodoItem } from "../../../lib/agentStore";
import { mergeToolReviewMeta } from "../toolReviewPresentation";
import type { ChatDisplayItem, ToolEventGroup } from "./types";

const HANDOFF_EVENT_MARKER = "[[handoff_event]]";

export type HandoffSystemEvent = {
  id?: string;
  kind?: "push" | "return";
  from_agent_id?: string;
  from_agent_name?: string;
  to_agent_id?: string;
  to_agent_name?: string;
  requested_by?: "user" | "agent";
  reason?: string;
  summary?: string;
  linked_thread_id?: string | null;
  approval_id?: string | null;
  stack_depth_before?: number;
  stack_depth_after?: number;
  created_at?: number;
};

export function parseHandoffSystemEvent(content: string): HandoffSystemEvent | null {
  if (!content.startsWith(HANDOFF_EVENT_MARKER)) {
    return null;
  }
  const payloadText = content
    .slice(HANDOFF_EVENT_MARKER.length)
    .split("\n", 1)[0]
    ?.trim();
  if (!payloadText) {
    return null;
  }
  try {
    const parsed = JSON.parse(payloadText);
    return parsed && typeof parsed === "object" ? parsed as HandoffSystemEvent : null;
  } catch {
    return null;
  }
}

export function buildDisplayItems(messages: AgentMessage[]): ChatDisplayItem[] {
  const items: ChatDisplayItem[] = [];
  const groups = new Map<string, ToolEventGroup>();

  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (isToolPlaceholderAssistantMessage(message, messages[index - 1], messages[index + 1])) {
      continue;
    }

    if (message.role !== "tool") {
      items.push({ type: "message", message });
      continue;
    }

    const groupKey = message.toolCallId || message.id;
    const existing = groups.get(groupKey);

    if (!existing) {
      const initialGroup: ToolEventGroup = {
        key: groupKey,
        toolCallId: message.toolCallId || message.id,
        toolName: message.toolName || "tool",
        toolArguments: message.toolArguments || "",
        status: message.toolStatus || (message.content ? "done" : "requested"),
        resultContent: message.content || "",
        createdAt: message.createdAt,
        welesReview: message.welesReview,
      };
      groups.set(groupKey, initialGroup);
      items.push({ type: "tool", group: initialGroup });
      continue;
    }

    if (message.toolName) existing.toolName = message.toolName;
    if (message.toolArguments) existing.toolArguments = message.toolArguments;
    if (message.toolStatus) {
      existing.status = message.toolStatus;
    } else if (message.content) {
      existing.status = "done";
    }
    if (message.content) existing.resultContent = message.content;
    existing.welesReview = mergeToolReviewMeta(existing.welesReview, message.welesReview);
    existing.createdAt = Math.min(existing.createdAt, message.createdAt);
  }

  return items;
}

function isToolPlaceholderAssistantMessage(
  message: AgentMessage,
  previous?: AgentMessage,
  next?: AgentMessage,
): boolean {
  if (message.role !== "assistant") {
    return false;
  }

  if (message.reasoning?.trim()) {
    return false;
  }

  const content = message.content.trim();
  if (content !== "" && content !== "Calling tools...") {
    return false;
  }

  return previous?.role === "tool" || next?.role === "tool";
}

export function filterDisplayItems(items: ChatDisplayItem[], searchQuery: string): ChatDisplayItem[] {
  const normalizedQuery = searchQuery.trim().toLowerCase();

  return items.filter((item) => {
    if (!normalizedQuery) {
      return true;
    }

    if (item.type === "message") {
      const message = item.message;
      return [
        message.role,
        message.content,
        message.reasoning ?? "",
        message.provider ?? "",
        message.model ?? "",
      ].join(" ").toLowerCase().includes(normalizedQuery);
    }

    return [
      item.group.toolName,
      item.group.toolArguments,
      item.group.resultContent,
      item.group.status,
    ].join(" ").toLowerCase().includes(normalizedQuery);
  });
}

export function summarizeSessionUsage(messages: AgentMessage[]) {
  let totalCost = 0;
  let hasCost = false;
  let tpsSum = 0;
  let tpsCount = 0;

  for (const message of messages) {
    if (message.role !== "assistant") continue;
    if (typeof message.cost === "number" && Number.isFinite(message.cost)) {
      totalCost += message.cost;
      hasCost = true;
    }
    if (typeof message.tps === "number" && Number.isFinite(message.tps) && message.tps > 0) {
      tpsSum += message.tps;
      tpsCount += 1;
    }
  }

  return {
    hasCost,
    totalCost,
    avgTps: tpsCount > 0 ? (tpsSum / tpsCount) : undefined,
  };
}

export function buildTodoPreview(todos: AgentTodoItem[]): string {
  return todos
    .slice()
    .sort((a, b) => a.position - b.position)
    .slice(0, 2)
    .map((item) => item.content)
    .join(" • ");
}

export function todoStatusColor(status: AgentTodoItem["status"]): string {
  switch (status) {
    case "in_progress":
      return "var(--accent)";
    case "completed":
      return "var(--success)";
    case "blocked":
      return "var(--warning)";
    default:
      return "var(--text-muted)";
  }
}
