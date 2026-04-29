import type { Dispatch, SetStateAction } from "react";
import { buildHydratedRemoteMessage, buildHydratedRemoteThread, useAgentStore } from "@/lib/agentStore";
import type {
  AgentMessage,
  AgentProviderConfig,
  AgentThread,
  AgentTodoItem,
  RemoteAgentMessageRecord,
} from "@/lib/agentStore";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { getAgentBridge } from "@/lib/agentDaemonConfig";
import { fetchThreadTodos } from "@/lib/agentTodos";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";
import type { GoalRun } from "@/lib/goalRuns";
import type { Workspace } from "@/lib/types";
import type { WelesHealthState } from "@/lib/agentStore/types";
import { findTaskWorkspaceLocation } from "../tasks-view/helpers";
import { formatSkillWorkflowNotice } from "./skillWorkflowNotice";

type RemoteAgentThread = {
  id: string;
  title: string;
  messages: RemoteAgentMessageRecord[];
  total_message_count?: number | null;
  loaded_message_start?: number | null;
  loaded_message_end?: number | null;
};

export function normalizeBridgePayload(payload: any) {
  if (payload && typeof payload === "object" && "data" in payload) {
    return payload.data ?? {};
  }
  return payload ?? {};
}

export function appendDaemonSystemMessage(content: string, threadId: string | null) {
  if (!threadId) return;
  useAgentStore.getState().addMessage(threadId, {
    role: "system",
    content,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  });
}

export function recordDaemonWorkflowNotice({
  event,
  activePaneId,
  activeWorkspace,
}: {
  event: any;
  activePaneId: string | null;
  activeWorkspace: ReturnType<ReturnType<typeof useWorkspaceStore.getState>["activeWorkspace"]>;
}) {
  const daemonThreadId = typeof event?.thread_id === "string" ? event.thread_id : null;
  const localThreadId = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === daemonThreadId)?.id ?? null;
  const thread = localThreadId
    ? useAgentStore.getState().threads.find((entry) => entry.id === localThreadId)
    : undefined;
  const paneId = thread?.paneId ?? activePaneId ?? "agent";
  const workspaceId = thread?.workspaceId ?? activeWorkspace?.id ?? null;
  const surfaceId = thread?.surfaceId ?? activeWorkspace?.surfaces?.[0]?.id ?? null;
  const rawKind = typeof event?.kind === "string" ? event.kind : "tool-call";
  const rawMessage = typeof event?.message === "string" ? event.message : null;
  const details = typeof event?.details === "string" ? event.details : null;
  const normalized = formatSkillWorkflowNotice(rawKind, rawMessage, details);
  const kind = normalized.kind;
  const message = normalized.message;

  if (kind === "transport-fallback" && details) {
    try {
      const parsed = JSON.parse(details);
      const provider = typeof parsed?.provider === "string" ? parsed.provider : null;
      const toTransport = parsed?.to === "chat_completions" ? "chat_completions" : null;
      if (provider && toTransport) {
        const currentSettings = useAgentStore.getState().agentSettings;
        const currentConfig = currentSettings[provider as keyof typeof currentSettings];
        if (currentConfig && typeof currentConfig === "object" && "base_url" in currentConfig) {
          useAgentStore.getState().updateAgentSetting(
            provider as keyof typeof currentSettings,
            {
              ...(currentConfig as AgentProviderConfig),
              api_transport: toTransport,
            } as any,
          );
        }
      }
    } catch {
      // Best-effort notice handling.
    }
  }

  useAgentMissionStore.getState().recordOperationalEvent({
    paneId,
    workspaceId,
    surfaceId,
    sessionId: daemonThreadId,
    kind: kind as any,
    command: kind,
    message: message ?? (details ? details : null),
  });
}

export async function reloadDaemonThreadIntoLocalState({
  daemonThreadId,
  setThreadTodos,
  setDaemonTodosByThread,
}: {
  daemonThreadId: string;
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
}) {
  await loadDaemonThreadPageIntoLocalState({
    daemonThreadId,
    mergeMode: "replace",
    setThreadTodos,
    setDaemonTodosByThread,
  });
}

export async function refreshDaemonThreadMetadataIntoLocalState({
  daemonThreadId,
  setThreadTodos,
  setDaemonTodosByThread,
}: {
  daemonThreadId: string;
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
}) {
  return loadDaemonThreadPageIntoLocalState({
    daemonThreadId,
    messageLimit: 0,
    messageOffset: 0,
    mergeMode: "metadata",
    setThreadTodos,
    setDaemonTodosByThread,
  });
}

export async function loadDaemonThreadPageIntoLocalState({
  daemonThreadId,
  localThreadId: requestedLocalThreadId,
  messageLimit,
  messageOffset,
  mergeMode,
  setThreadTodos,
  setDaemonTodosByThread,
}: {
  daemonThreadId: string;
  localThreadId?: string | null;
  messageLimit?: number | null;
  messageOffset?: number | null;
  mergeMode: "replace" | "prepend" | "metadata";
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
}): Promise<boolean> {
  const zorai = getAgentBridge();
  if (!zorai?.agentGetThread) return false;

  const stateBeforeLoad = useAgentStore.getState();
  const localThreadId = requestedLocalThreadId
    ?? stateBeforeLoad.threads.find(
      (thread) => thread.id === stateBeforeLoad.activeThreadId && thread.daemonThreadId === daemonThreadId,
    )?.id
    ?? stateBeforeLoad.threads.find(
      (thread) => thread.daemonThreadId === daemonThreadId,
    )?.id;
  if (!localThreadId) return false;

  const remotePayload = await zorai.agentGetThread(daemonThreadId, {
    messageLimit: messageLimit ?? resolveReactChatHistoryMessageLimit(
      useAgentStore.getState().agentSettings.react_chat_history_page_size,
    ) ?? null,
    messageOffset: messageOffset ?? null,
  }).catch(() => null) as any;
  const remoteThread = normalizeBridgePayload(remotePayload);
  const hydrated = buildHydratedRemoteThread(
    (remoteThread ?? {}) as any,
    remoteThread?.agent_name ?? "assistant",
  );
  if (!hydrated) return false;

  const reloadedThread = {
    ...hydrated.thread,
    id: localThreadId,
    daemonThreadId,
  } as AgentThread;
  const reloadedMessages = hydrated.messages.map((message) => ({
    ...message,
    threadId: localThreadId,
  })) as AgentMessage[];

  useAgentStore.setState((state) => ({
    threads: state.threads.map((thread) => thread.id === localThreadId ? {
      ...thread,
      ...reloadedThread,
      lastMessagePreview: reloadedThread.lastMessagePreview || thread.lastMessagePreview,
      loadedMessageStart: mergeMode === "prepend"
        ? Math.min(thread.loadedMessageStart ?? reloadedThread.loadedMessageStart ?? 0, reloadedThread.loadedMessageStart ?? 0)
        : reloadedThread.loadedMessageStart,
      loadedMessageEnd: mergeMode === "prepend"
        ? Math.max(thread.loadedMessageEnd ?? 0, reloadedThread.loadedMessageEnd ?? 0)
        : reloadedThread.loadedMessageEnd,
    } : thread),
    messages: mergeMode === "metadata"
      ? state.messages
      : {
        ...state.messages,
        [localThreadId]: mergeMode === "prepend"
          ? mergeMessages(reloadedMessages, state.messages[localThreadId] ?? [])
          : reloadedMessages,
      },
  }));

  const todos = await fetchThreadTodos(daemonThreadId).catch(() => []);
  setThreadTodos(localThreadId, todos);
  setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: todos }));
  return true;
}

export function trimDaemonThreadMessagesToLatestWindow({
  localThreadId,
  messageLimit,
}: {
  localThreadId: string;
  messageLimit?: number | null;
}): boolean {
  if (!Number.isFinite(messageLimit) || (messageLimit ?? 0) <= 0) {
    return false;
  }

  const limit = Math.floor(messageLimit as number);
  let didTrim = false;
  useAgentStore.setState((state) => {
    const currentMessages = state.messages[localThreadId] ?? [];
    if (currentMessages.length <= limit) {
      return {};
    }

    const thread = state.threads.find((entry) => entry.id === localThreadId);
    if (!thread) {
      return {};
    }

    const keptMessages = currentMessages.slice(-limit);
    const loadedMessageEnd = thread.loadedMessageEnd ?? thread.messageCount ?? currentMessages.length;
    const loadedMessageStart = Math.max(0, loadedMessageEnd - keptMessages.length);
    didTrim = true;

    return {
      threads: state.threads.map((entry) => entry.id === localThreadId ? {
        ...entry,
        loadedMessageStart,
        loadedMessageEnd,
      } : entry),
      messages: {
        ...state.messages,
        [localThreadId]: keptMessages,
      },
    };
  });

  return didTrim;
}

function mergeMessages(prefix: AgentMessage[], existing: AgentMessage[]): AgentMessage[] {
  const seen = new Set<string>();
  const merged: AgentMessage[] = [];
  for (const message of [...prefix, ...existing]) {
    if (seen.has(message.id)) continue;
    seen.add(message.id);
    merged.push(message);
  }
  return merged;
}

export async function hydrateDaemonThreadIntoLocalState({
  sessionId,
  fallbackTitle,
  workspaces,
  remoteThread,
  fetchThreadTodos: fetchThreadTodosForThread,
  createThread,
  addMessage,
  setThreadDaemonId,
  setThreadTodos,
  onThreadReady,
}: {
  sessionId?: string | null;
  fallbackTitle: string;
  workspaces: Workspace[];
  remoteThread: RemoteAgentThread;
  fetchThreadTodos: (threadId: string) => Promise<AgentTodoItem[]>;
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  onThreadReady?: (localThreadId: string, remoteThreadId: string) => void;
}): Promise<string | null> {
  const location = findTaskWorkspaceLocation(workspaces, sessionId);
  const localThreadId = createThread({
    workspaceId: location?.workspaceId ?? null,
    surfaceId: location?.surfaceId ?? null,
    paneId: location?.paneId ?? null,
    title: remoteThread.title || fallbackTitle,
  });
  setThreadDaemonId(localThreadId, remoteThread.id);
  onThreadReady?.(localThreadId, remoteThread.id);

  for (const message of remoteThread.messages ?? []) {
    addMessage(localThreadId, buildHydratedRemoteMessage(localThreadId, message));
  }

  const todos = await fetchThreadTodosForThread(remoteThread.id).catch(() => []);
  setThreadTodos(localThreadId, todos);
  return localThreadId;
}

export function syncWelesHealth(
  event: any,
  setWelesHealth: Dispatch<SetStateAction<WelesHealthState | null>>,
  appendSystemMessage: (content: string) => void,
) {
  const state = typeof event.state === "string" ? event.state : "healthy";
  const reason = typeof event.reason === "string" ? event.reason : undefined;
  const checkedAt = typeof event.checked_at === "number" ? event.checked_at : Date.now();
  const nextHealth = { state, reason, checkedAt };
  setWelesHealth(nextHealth);
  if (state === "degraded") {
    appendSystemMessage(`WELES degraded\n\n${reason || "Daemon vitality checks require attention."}`);
  }
}

export function refreshGoalRuns(setGoalRunsForTrace: Dispatch<SetStateAction<GoalRun[]>>) {
  return (runs: GoalRun[]) => setGoalRunsForTrace(runs);
}
