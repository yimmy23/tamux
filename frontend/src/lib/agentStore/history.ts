import { readPersistedJson, scheduleJsonWrite } from "../persistence";
import { getBridge } from "../bridge";
import { PRIMARY_AGENT_NAME } from "../agentNames";
import { normalizeAgentProviderId, normalizeApiTransport } from "./providers";
import type { AgentSettings } from "./settings";
import type {
  AgentMessage,
  AgentProviderId,
  AgentRole,
  AgentThread,
  AgentTodoItem,
  ProviderAuthState,
} from "./types";

export const AGENT_CHAT_FILE = "agent-chat.json";
export const AGENT_DAEMON_THREAD_MAP_FILE = "agent-daemon-thread-map.json";
export const AGENT_ACTIVE_THREAD_FILE = "agent-active-thread.json";

export type AgentChatState = {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>;
  todos: Record<string, AgentTodoItem[]>;
  activeThreadId: string | null;
};

type AgentDbThreadRecord = {
  id: string;
  workspace_id: string | null;
  surface_id: string | null;
  pane_id: string | null;
  agent_name: string | null;
  title: string;
  created_at: number;
  updated_at: number;
  message_count: number;
  total_tokens: number;
  last_preview: string;
  metadata_json: string | null;
};

type AgentDbMessageRecord = {
  id: string;
  thread_id: string;
  created_at: number;
  role: string;
  content: string;
  provider: string | null;
  model: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  reasoning: string | null;
  tool_calls_json: string | null;
  metadata_json: string | null;
};

export type RemoteAgentMessageRecord = {
  role?: AgentRole;
  content?: string;
  provider?: string | null;
  model?: string | null;
  api_transport?: string | null;
  response_id?: string | null;
  provider_final_result?: unknown;
  tool_calls?: AgentMessage["toolCalls"] | null;
  tool_name?: string | null;
  tool_call_id?: string | null;
  tool_arguments?: string | null;
  tool_status?: string | null;
  weles_review?: AgentMessage["welesReview"] | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  reasoning?: string | null;
  timestamp?: number | null;
  message_kind?: "normal" | "compaction_artifact";
  compaction_strategy?: "heuristic" | "weles" | "custom_model" | null;
  compaction_payload?: string | null;
};

export type RemoteAgentThreadRecord = {
  id?: string;
  agent_name?: string | null;
  title?: string;
  messages?: RemoteAgentMessageRecord[];
  upstream_thread_id?: string | null;
  upstream_transport?: string | null;
  upstream_provider?: string | null;
  upstream_model?: string | null;
  upstream_assistant_id?: string | null;
  created_at?: number | null;
  updated_at?: number | null;
  total_input_tokens?: number | null;
  total_output_tokens?: number | null;
};

export function isHiddenAgentThread(thread: Pick<RemoteAgentThreadRecord, "id" | "title">): boolean {
  const id = typeof thread.id === "string" ? thread.id.trim().toLowerCase() : "";
  const title = typeof thread.title === "string" ? thread.title.trim().toLowerCase() : "";
  return id.startsWith("dm:")
    || id.startsWith("handoff:")
    || title.startsWith("internal dm")
    || title.startsWith("handoff ")
    || title === "weles"
    || title.startsWith("weles ");
}

type AgentDbApi = {
  dbCreateThread?: (thread: AgentDbThreadRecord) => Promise<boolean>;
  dbDeleteThread?: (id: string) => Promise<boolean>;
  dbListThreads?: () => Promise<AgentDbThreadRecord[]>;
  dbGetThread?: (id: string) => Promise<{ thread: AgentDbThreadRecord | null; messages: AgentDbMessageRecord[] }>;
  dbAddMessage?: (message: AgentDbMessageRecord) => Promise<boolean>;
  dbDeleteMessage?: (threadId: string, messageId: string) => Promise<boolean>;
  dbListMessages?: (threadId: string, limit?: number | null) => Promise<AgentDbMessageRecord[]>;
};

const threadAbortControllers = new Map<string, AbortController>();

let threadIdCounter = 0;
let messageIdCounter = 0;

export function nextThreadId(): string {
  threadIdCounter += 1;
  return `thread_${threadIdCounter}`;
}

export function nextMessageId(): string {
  messageIdCounter += 1;
  return `msg_${messageIdCounter}`;
}

export function setThreadAbortController(threadId: string, controller: AbortController): void {
  threadAbortControllers.set(threadId, controller);
}

export function abortThreadStream(threadId: string): void {
  const controller = threadAbortControllers.get(threadId);
  if (!controller) {
    return;
  }
  controller.abort();
  threadAbortControllers.delete(threadId);
}

export function clearThreadAbortController(threadId: string, controller?: AbortController): void {
  const current = threadAbortControllers.get(threadId);
  if (!current) {
    return;
  }
  if (controller && current !== controller) {
    return;
  }
  threadAbortControllers.delete(threadId);
}

export function getAgentDbApi(): AgentDbApi | null {
  const api = getBridge();
  if (!api) {
    return null;
  }
  return api as AgentDbApi;
}

export function shouldPersistHistory(backend: AgentSettings["agent_backend"]): boolean {
  const bridge = getBridge();
  return backend === "legacy" && !bridge?.agentSendMessage;
}

export function buildHydratedRemoteMessage(
  threadId: string,
  message: RemoteAgentMessageRecord,
): AgentMessage {
  const provider = typeof message.provider === "string" ? message.provider : undefined;
  return {
    id: nextMessageId(),
    threadId,
    createdAt: Number(message.timestamp ?? Date.now()),
    role: message.role ?? "assistant",
    content: typeof message.content === "string" ? message.content : "",
    provider,
    model: typeof message.model === "string" ? message.model : undefined,
    api_transport: typeof message.api_transport === "string"
      ? normalizeApiTransport(
        typeof provider === "string" ? normalizeAgentProviderId(provider) : "openai",
        message.api_transport,
      )
      : undefined,
    responseId: typeof message.response_id === "string" ? message.response_id : undefined,
    providerFinalResult:
      message.provider_final_result && typeof message.provider_final_result === "object"
        ? message.provider_final_result
        : undefined,
    toolCalls: Array.isArray(message.tool_calls) ? message.tool_calls : undefined,
    toolName: typeof message.tool_name === "string" ? message.tool_name : undefined,
    toolCallId: typeof message.tool_call_id === "string" ? message.tool_call_id : undefined,
    toolArguments: typeof message.tool_arguments === "string" ? message.tool_arguments : undefined,
    toolStatus:
      message.tool_status === "requested"
        || message.tool_status === "executing"
        || message.tool_status === "done"
        || message.tool_status === "error"
        ? message.tool_status
        : undefined,
    welesReview: message.weles_review ?? undefined,
    inputTokens: Number(message.input_tokens ?? 0),
    outputTokens: Number(message.output_tokens ?? 0),
    totalTokens: Number(message.input_tokens ?? 0) + Number(message.output_tokens ?? 0),
    reasoning: typeof message.reasoning === "string" ? message.reasoning : undefined,
    isCompactionSummary: message.message_kind === "compaction_artifact",
    messageKind: message.message_kind ?? "normal",
    compactionStrategy: message.compaction_strategy ?? undefined,
    compactionPayload: typeof message.compaction_payload === "string" ? message.compaction_payload : undefined,
    isStreaming: false,
  };
}

export function buildHydratedRemoteThread(
  thread: RemoteAgentThreadRecord,
  agent_name: string,
): {
  thread: AgentThread;
  messages: AgentMessage[];
} | null {
  if (typeof thread.id !== "string" || !thread.id.trim()) {
    return null;
  }

  if (isHiddenAgentThread(thread)) {
    return null;
  }

  const localThreadId = nextThreadId();
  const messages = Array.isArray(thread.messages)
    ? thread.messages.map((message) => buildHydratedRemoteMessage(localThreadId, message))
    : [];
  const totalInputTokens = Number(thread.total_input_tokens ?? 0);
  const totalOutputTokens = Number(thread.total_output_tokens ?? 0);
  const resolvedAgentName = typeof thread.agent_name === "string" && thread.agent_name.trim()
    ? thread.agent_name
    : agent_name;

  return {
    thread: {
      id: localThreadId,
      daemonThreadId: thread.id,
      workspaceId: null,
      surfaceId: null,
      paneId: null,
      agent_name: resolvedAgentName,
      title: typeof thread.title === "string" && thread.title.trim()
        ? thread.title
        : "Conversation",
      createdAt: Number(thread.created_at ?? Date.now()),
      updatedAt: Number(thread.updated_at ?? Date.now()),
      messageCount: messages.length,
      totalInputTokens,
      totalOutputTokens,
      totalTokens: totalInputTokens + totalOutputTokens,
      compactionCount: messages.filter((message) => message.messageKind === "compaction_artifact").length,
      lastMessagePreview: messages[messages.length - 1]?.content?.slice(0, 100) ?? "",
      upstreamThreadId: typeof thread.upstream_thread_id === "string" ? thread.upstream_thread_id : null,
      upstreamTransport: typeof thread.upstream_transport === "string"
        ? normalizeApiTransport(
          typeof thread.upstream_provider === "string"
            ? normalizeAgentProviderId(thread.upstream_provider)
            : "openai",
          thread.upstream_transport,
        )
        : undefined,
      upstreamProvider: typeof thread.upstream_provider === "string"
        ? normalizeAgentProviderId(thread.upstream_provider)
        : null,
      upstreamModel: typeof thread.upstream_model === "string" ? thread.upstream_model : null,
      upstreamAssistantId: typeof thread.upstream_assistant_id === "string"
        ? thread.upstream_assistant_id
        : null,
    },
    messages,
  };
}

export function syncChatCounters(chat: AgentChatState): void {
  let maxThread = 0;
  let maxMessage = 0;

  for (const thread of chat.threads) {
    const match = /^thread_(\d+)$/.exec(thread.id);
    if (match) {
      maxThread = Math.max(maxThread, Number(match[1]));
    }
  }

  for (const threadMessages of Object.values(chat.messages)) {
    for (const message of threadMessages) {
      const match = /^msg_(\d+)$/.exec(message.id);
      if (match) {
        maxMessage = Math.max(maxMessage, Number(match[1]));
      }
    }
  }

  threadIdCounter = Math.max(threadIdCounter, maxThread);
  messageIdCounter = Math.max(messageIdCounter, maxMessage);
}

export function persistDaemonThreadMap(threads: AgentThread[]): void {
  const mapping = Object.fromEntries(
    threads
      .filter((thread) => typeof thread.daemonThreadId === "string" && thread.daemonThreadId)
      .map((thread) => [thread.id, thread.daemonThreadId]),
  );
  scheduleJsonWrite(AGENT_DAEMON_THREAD_MAP_FILE, mapping);
}

export function isValidProviderAuthStates(value: unknown): value is ProviderAuthState[] {
  return Array.isArray(value)
    && value.length > 0
    && value.every((entry) =>
      entry
      && typeof entry === "object"
      && typeof (entry as ProviderAuthState).provider_id === "string"
      && typeof (entry as ProviderAuthState).provider_name === "string");
}

export function serializeThread(thread: AgentThread): AgentDbThreadRecord {
  return {
    id: thread.id,
    workspace_id: thread.workspaceId ?? null,
    surface_id: thread.surfaceId ?? null,
    pane_id: thread.paneId ?? null,
    agent_name: thread.agent_name ?? null,
    title: thread.title,
    created_at: thread.createdAt,
    updated_at: thread.updatedAt,
    message_count: thread.messageCount,
    total_tokens: thread.totalTokens,
    last_preview: thread.lastMessagePreview,
    metadata_json: JSON.stringify({
      upstreamThreadId: thread.upstreamThreadId ?? null,
      upstreamTransport: thread.upstreamTransport ?? null,
      upstreamProvider: thread.upstreamProvider ?? null,
      upstreamModel: thread.upstreamModel ?? null,
      upstreamAssistantId: thread.upstreamAssistantId ?? null,
    }),
  };
}

export function serializeMessage(message: AgentMessage): AgentDbMessageRecord {
  return {
    id: message.id,
    thread_id: message.threadId,
    created_at: message.createdAt,
    role: message.role,
    content: message.content,
    provider: message.provider ?? null,
    model: message.model ?? null,
    input_tokens: message.inputTokens,
    output_tokens: message.outputTokens,
    total_tokens: message.totalTokens,
    reasoning: message.reasoning ?? null,
    tool_calls_json: message.toolCalls ? JSON.stringify(message.toolCalls) : null,
    metadata_json: JSON.stringify({
      toolName: message.toolName ?? null,
      toolCallId: message.toolCallId ?? null,
      toolArguments: message.toolArguments ?? null,
      toolStatus: message.toolStatus ?? null,
      welesReview: message.welesReview ?? null,
      api_transport: message.api_transport ?? null,
      responseId: message.responseId ?? null,
      providerFinalResult: message.providerFinalResult ?? null,
      reasoningTokens: message.reasoningTokens ?? null,
      audioTokens: message.audioTokens ?? null,
      videoTokens: message.videoTokens ?? null,
      cost: message.cost ?? null,
      tps: message.tps ?? null,
      isCompactionSummary: message.isCompactionSummary,
      messageKind: message.messageKind ?? "normal",
      compactionStrategy: message.compactionStrategy ?? null,
      compactionPayload: message.compactionPayload ?? null,
      isStreaming: message.isStreaming ?? false,
    }),
  };
}

export function deserializeThread(thread: AgentDbThreadRecord): AgentThread {
  let metadata: Record<string, unknown> = {};
  try {
    metadata = typeof thread.metadata_json === "string"
      ? JSON.parse(thread.metadata_json)
      : {};
  } catch {
    metadata = {};
  }
  return {
    id: thread.id,
    daemonThreadId: null,
    workspaceId: thread.workspace_id,
    surfaceId: thread.surface_id,
    paneId: thread.pane_id,
    agent_name: thread.agent_name ?? PRIMARY_AGENT_NAME,
    title: thread.title,
    createdAt: thread.created_at,
    updatedAt: thread.updated_at,
    messageCount: thread.message_count,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: thread.total_tokens,
    compactionCount: 0,
    lastMessagePreview: thread.last_preview,
    upstreamThreadId: typeof metadata.upstreamThreadId === "string" ? metadata.upstreamThreadId : null,
    upstreamTransport: typeof metadata.upstreamTransport === "string"
      ? normalizeApiTransport(
        typeof metadata.upstreamProvider === "string"
          ? normalizeAgentProviderId(metadata.upstreamProvider)
          : "openai",
        metadata.upstreamTransport,
      )
      : undefined,
    upstreamProvider: typeof metadata.upstreamProvider === "string"
      ? normalizeAgentProviderId(metadata.upstreamProvider as AgentProviderId)
      : null,
    upstreamModel: typeof metadata.upstreamModel === "string" ? metadata.upstreamModel : null,
    upstreamAssistantId: typeof metadata.upstreamAssistantId === "string" ? metadata.upstreamAssistantId : null,
  };
}

export function deserializeMessage(message: AgentDbMessageRecord): AgentMessage {
  let metadata: Record<string, unknown> = {};
  try {
    metadata = typeof message.metadata_json === "string"
      ? JSON.parse(message.metadata_json)
      : {};
  } catch {
    metadata = {};
  }
  let toolCalls: AgentMessage["toolCalls"];
  try {
    toolCalls = typeof message.tool_calls_json === "string" ? JSON.parse(message.tool_calls_json) : undefined;
  } catch {
    toolCalls = undefined;
  }
  return {
    id: message.id,
    threadId: message.thread_id,
    createdAt: message.created_at,
    role: message.role as AgentRole,
    content: message.content,
    provider: message.provider ?? undefined,
    model: message.model ?? undefined,
    api_transport: typeof metadata.api_transport === "string"
      ? normalizeApiTransport(
        typeof message.provider === "string"
          ? normalizeAgentProviderId(message.provider)
          : "openai",
        metadata.api_transport,
      )
      : undefined,
    responseId: typeof metadata.responseId === "string" ? metadata.responseId : undefined,
    providerFinalResult:
      metadata.providerFinalResult && typeof metadata.providerFinalResult === "object"
        ? metadata.providerFinalResult
        : undefined,
    toolCalls,
    toolName: (metadata.toolName as string) ?? undefined,
    toolCallId: (metadata.toolCallId as string) ?? undefined,
    toolArguments: (metadata.toolArguments as string) ?? undefined,
    toolStatus: (metadata.toolStatus as AgentMessage["toolStatus"]) ?? undefined,
    welesReview: (metadata.welesReview as AgentMessage["welesReview"]) ?? undefined,
    inputTokens: message.input_tokens ?? 0,
    outputTokens: message.output_tokens ?? 0,
    totalTokens: message.total_tokens ?? 0,
    reasoning: message.reasoning ?? undefined,
    reasoningTokens: (metadata.reasoningTokens as number) ?? undefined,
    audioTokens: (metadata.audioTokens as number) ?? undefined,
    videoTokens: (metadata.videoTokens as number) ?? undefined,
    cost: (metadata.cost as number) ?? undefined,
    tps: (metadata.tps as number) ?? undefined,
    isCompactionSummary: Boolean(metadata.isCompactionSummary),
    messageKind:
      metadata.messageKind === "compaction_artifact" ? "compaction_artifact" : "normal",
    compactionStrategy:
      metadata.compactionStrategy === "heuristic"
        || metadata.compactionStrategy === "weles"
        || metadata.compactionStrategy === "custom_model"
        ? metadata.compactionStrategy
        : undefined,
    compactionPayload:
      typeof metadata.compactionPayload === "string"
        ? metadata.compactionPayload
        : undefined,
    isStreaming: Boolean(metadata.isStreaming),
  };
}

export { readPersistedJson };
