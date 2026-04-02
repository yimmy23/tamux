import type { AgentTodoItem } from "./agentStore";
import { getBridge } from "./bridge";

function normalizeTodo(raw: unknown, index: number): AgentTodoItem | null {
  const item = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
  const content = typeof item?.content === "string" ? item.content.trim() : "";
  if (!content) return null;

  return {
    id: typeof item?.id === "string" && item.id ? item.id : `todo-${index}`,
    content,
    status:
      item?.status === "in_progress" || item?.status === "completed" || item?.status === "blocked"
        ? item.status
        : "pending",
    position: typeof item?.position === "number" ? item.position : index,
    stepIndex:
      typeof item?.step_index === "number"
        ? item.step_index
        : typeof item?.stepIndex === "number"
          ? item.stepIndex
          : null,
    createdAt:
      typeof item?.created_at === "number"
        ? item.created_at
        : typeof item?.createdAt === "number"
          ? item.createdAt
          : null,
    updatedAt:
      typeof item?.updated_at === "number"
        ? item.updated_at
        : typeof item?.updatedAt === "number"
          ? item.updatedAt
          : null,
  };
}

function normalizeTodoList(raw: unknown): AgentTodoItem[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((item, index) => normalizeTodo(item, index))
    .filter((item): item is AgentTodoItem => Boolean(item))
    .sort((a, b) => a.position - b.position);
}

export async function fetchThreadTodos(threadId: string): Promise<AgentTodoItem[]> {
  const bridge = getBridge();
  if (!bridge?.agentGetTodos || !threadId) return [];

  try {
    const result = await bridge.agentGetTodos(threadId);
    if (Array.isArray(result)) return normalizeTodoList(result);
    if (result && typeof result === "object") {
      const payload = result as Record<string, unknown>;
      return normalizeTodoList(payload.items);
    }
    return [];
  } catch {
    return [];
  }
}

export async function fetchAllThreadTodos(): Promise<Record<string, AgentTodoItem[]>> {
  const bridge = getBridge();
  if (!bridge?.agentListTodos) return {};

  try {
    const result = await bridge.agentListTodos();
    if (!result || typeof result !== "object" || Array.isArray(result)) return {};
    return Object.fromEntries(
      Object.entries(result as Record<string, unknown>).map(([threadId, items]) => [
        threadId,
        normalizeTodoList(items),
      ]),
    );
  } catch {
    return {};
  }
}
