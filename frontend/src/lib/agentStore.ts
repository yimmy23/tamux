import { create } from "zustand";
import type { WorkspaceId, SurfaceId, PaneId } from "./types";
import type { ToolCall } from "./agentTools";
import { readPersistedJson, scheduleJsonWrite } from "./persistence";

// ---------------------------------------------------------------------------
// Types matching amux-windows AgentConversationThread/Message
// ---------------------------------------------------------------------------
export interface AgentThread {
  id: string;
  workspaceId: WorkspaceId | null;
  surfaceId: SurfaceId | null;
  paneId: PaneId | null;
  agentName: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalTokens: number;
  compactionCount: number;
  lastMessagePreview: string;
}

export type AgentRole = "user" | "assistant" | "system" | "tool";

export type AgentProviderId =
  | "featherless"
  | "openai"
  | "anthropic"
  | "qwen"
  | "qwen-deepinfra"
  | "kimi"
  | "z.ai"
  | "openrouter"
  | "cerebras"
  | "together"
  | "groq"
  | "ollama"
  | "chutes"
  | "huggingface"
  | "minimax"
  | "custom";

const AGENT_PROVIDER_IDS: AgentProviderId[] = [
  "featherless",
  "openai",
  "anthropic",
  "qwen",
  "qwen-deepinfra",
  "kimi",
  "z.ai",
  "openrouter",
  "cerebras",
  "together",
  "groq",
  "ollama",
  "chutes",
  "huggingface",
  "minimax",
  "custom",
];

function normalizeAgentProviderId(value: unknown): AgentProviderId {
  if (typeof value !== "string") {
    return "openai";
  }
  const normalized = value.trim() as AgentProviderId;
  return AGENT_PROVIDER_IDS.includes(normalized) ? normalized : "openai";
}

const VALID_AGENT_BACKENDS = ["daemon", "openclaw", "hermes", "legacy"] as const;

function normalizeAgentBackend(value: unknown): AgentSettings["agentBackend"] {
  if (typeof value === "string" && (VALID_AGENT_BACKENDS as readonly string[]).includes(value)) {
    return value as AgentSettings["agentBackend"];
  }
  return "daemon";
}

export interface AgentProviderConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
}

export interface AgentMessage {
  id: string;
  threadId: string;
  createdAt: number;
  role: AgentRole;
  content: string;
  provider?: string;
  model?: string;
  toolCalls?: ToolCall[];
  toolName?: string;
  toolCallId?: string;
  toolArguments?: string;
  toolStatus?: "requested" | "executing" | "done" | "error";
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reasoning?: string;
  reasoningTokens?: number;
  audioTokens?: number;
  videoTokens?: number;
  cost?: number;
  tps?: number;
  isCompactionSummary: boolean;
  isStreaming?: boolean;
}

// ---------------------------------------------------------------------------
// Agent Settings matching amux-windows
// ---------------------------------------------------------------------------
export interface AgentSettings {
  enabled: boolean;
  agentName: string;
  handler: string;
  additionalHandlers: string[];
  systemPrompt: string;

  activeProvider: AgentProviderId;
  featherless: AgentProviderConfig;
  openai: AgentProviderConfig;
  anthropic: AgentProviderConfig;
  qwen: AgentProviderConfig;
  "qwen-deepinfra": AgentProviderConfig;
  kimi: AgentProviderConfig;
  "z.ai": AgentProviderConfig;
  openrouter: AgentProviderConfig;
  cerebras: AgentProviderConfig;
  together: AgentProviderConfig;
  groq: AgentProviderConfig;
  ollama: AgentProviderConfig;
  chutes: AgentProviderConfig;
  huggingface: AgentProviderConfig;
  minimax: AgentProviderConfig;
  custom: AgentProviderConfig;

  enableBashTool: boolean;
  enableVisionTool: boolean;
  enableWebBrowsingTool: boolean;
  bashTimeoutSeconds: number;
  enableWebSearchTool: boolean;
  searchToolProvider: "none" | "firecrawl" | "exa" | "tavily";
  firecrawlApiKey: string;
  exaApiKey: string;
  tavilyApiKey: string;
  searchMaxResults: number;
  searchTimeoutSeconds: number;

  enableStreaming: boolean;
  enableConversationMemory: boolean;
  enableHonchoMemory: boolean;
  honchoApiKey: string;
  honchoBaseUrl: string;
  honchoWorkspaceId: string;

  chatFontFamily: string;
  chatFontSize: number;

  autoCompactContext: boolean;
  maxContextMessages: number;
  maxToolLoops: number;
  maxRetries: number;
  retryDelayMs: number;
  contextBudgetTokens: number;
  compactThresholdPercent: number;
  keepRecentOnCompaction: number;

  // Agent backend: "daemon" runs LLM in tamux-daemon, "openclaw"/"hermes" route
  // through external agent processes, "legacy" uses frontend
  agentBackend: "daemon" | "openclaw" | "hermes" | "legacy";
}

export const DEFAULT_AGENT_SETTINGS: AgentSettings = {
  enabled: false,
  agentName: "assistant",
  handler: "/agent",
  additionalHandlers: [],
  systemPrompt: "You are tamux, an agentic terminal multiplexer assistant. You can execute terminal commands, check system resources, and send messages to connected chat platforms (Slack, Discord, Telegram, WhatsApp) via the gateway. Use your tools proactively when the user asks you to perform actions. Be concise and direct.",

  activeProvider: "openai",
  featherless: { baseUrl: "https://api.featherless.ai/v1", model: "meta-llama/Llama-3.3-70B-Instruct", apiKey: "" },
  openai: { baseUrl: "https://api.openai.com/v1", model: "gpt-4o", apiKey: "" },
  anthropic: { baseUrl: "https://api.anthropic.com", model: "claude-sonnet-4-20250514", apiKey: "" },
  qwen: { baseUrl: "https://api.qwen.com/v1", model: "qwen-max", apiKey: "" },
  "qwen-deepinfra": { baseUrl: "https://api.deepinfra.com/v1/openai", model: "Qwen/Qwen2.5-72B-Instruct", apiKey: "" },
  kimi: { baseUrl: "https://api.moonshot.ai/v1", model: "moonshot-v1-32k", apiKey: "" },
  "z.ai": { baseUrl: "https://api.z.ai/api/paas/v4", model: "glm-4-plus", apiKey: "" },
  openrouter: { baseUrl: "https://openrouter.ai/api/v1", model: "anthropic/claude-sonnet-4", apiKey: "" },
  cerebras: { baseUrl: "https://api.cerebras.ai/v1", model: "llama-3.3-70b", apiKey: "" },
  together: { baseUrl: "https://api.together.xyz/v1", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", apiKey: "" },
  groq: { baseUrl: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", apiKey: "" },
  ollama: { baseUrl: "http://localhost:11434/v1", model: "llama3.1", apiKey: "" },
  chutes: { baseUrl: "https://llm.chutes.ai/v1", model: "deepseek-ai/DeepSeek-V3", apiKey: "" },
  huggingface: { baseUrl: "https://api-inference.huggingface.co/v1", model: "meta-llama/Llama-3.3-70B-Instruct", apiKey: "" },
  minimax: { baseUrl: "https://api.minimax.io/anthropic", model: "MiniMax-M1-80k", apiKey: "" },
  custom: { baseUrl: "", model: "", apiKey: "" },

  enableBashTool: true,
  enableVisionTool: false,
  enableWebBrowsingTool: false,
  bashTimeoutSeconds: 30,
  enableWebSearchTool: false,
  searchToolProvider: "none",
  firecrawlApiKey: "",
  exaApiKey: "",
  tavilyApiKey: "",
  searchMaxResults: 8,
  searchTimeoutSeconds: 20,

  enableStreaming: true,
  enableConversationMemory: true,
  enableHonchoMemory: false,
  honchoApiKey: "",
  honchoBaseUrl: "",
  honchoWorkspaceId: "tamux",

  chatFontFamily: "Cascadia Code",
  chatFontSize: 13,

  autoCompactContext: true,
  maxContextMessages: 100,
  maxToolLoops: 25,
  maxRetries: 3,
  retryDelayMs: 2000,
  contextBudgetTokens: 100000,
  compactThresholdPercent: 80,
  keepRecentOnCompaction: 10,

  agentBackend: "daemon",
};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------
export interface AgentState {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>; // threadId -> messages
  activeThreadId: string | null;
  agentPanelOpen: boolean;
  agentSettings: AgentSettings;
  searchQuery: string;

  // Thread actions
  createThread: (opts: {
    workspaceId?: WorkspaceId | null;
    surfaceId?: SurfaceId | null;
    paneId?: PaneId | null;
    title?: string;
  }) => string;
  deleteThread: (id: string) => void;
  setActiveThread: (id: string | null) => void;
  searchThreads: (query: string) => AgentThread[];

  // Message actions
  addMessage: (threadId: string, msg: Omit<AgentMessage, "id" | "threadId" | "createdAt">) => void;
  updateLastAssistantMessage: (
    threadId: string,
    content: string,
    streaming?: boolean,
    meta?: Partial<Pick<AgentMessage, "inputTokens" | "outputTokens" | "totalTokens" | "reasoning" | "reasoningTokens" | "audioTokens" | "videoTokens" | "cost" | "tps" | "toolCalls" | "provider" | "model">>
  ) => void;
  getThreadMessages: (threadId: string) => AgentMessage[];

  // Panel
  toggleAgentPanel: () => void;
  setSearchQuery: (q: string) => void;

  // Settings
  updateAgentSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
  resetAgentSettings: () => void;

  // Derived
  getThreadsForPane: (paneId: PaneId) => AgentThread[];
}

const threadAbortControllers = new Map<string, AbortController>();

export function setThreadAbortController(threadId: string, controller: AbortController): void {
  threadAbortControllers.set(threadId, controller);
}

export function abortThreadStream(threadId: string): void {
  const controller = threadAbortControllers.get(threadId);
  if (!controller) return;
  controller.abort();
  threadAbortControllers.delete(threadId);
}

export function clearThreadAbortController(threadId: string, controller?: AbortController): void {
  const current = threadAbortControllers.get(threadId);
  if (!current) return;
  if (controller && current !== controller) return;
  threadAbortControllers.delete(threadId);
}

let _threadId = 0;
let _msgId = 0;

function loadAgentSettings(): AgentSettings {
  return { ...DEFAULT_AGENT_SETTINGS };
}

const AGENT_SETTINGS_FILE = "agent-settings.json";
const AGENT_CHAT_FILE = "agent-chat.json";

type AgentChatState = {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>;
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

type AgentDbApi = {
  dbCreateThread?: (thread: AgentDbThreadRecord) => Promise<boolean>;
  dbDeleteThread?: (id: string) => Promise<boolean>;
  dbListThreads?: () => Promise<AgentDbThreadRecord[]>;
  dbGetThread?: (id: string) => Promise<{ thread: AgentDbThreadRecord | null; messages: AgentDbMessageRecord[] }>;
  dbAddMessage?: (message: AgentDbMessageRecord) => Promise<boolean>;
  dbListMessages?: (threadId: string, limit?: number | null) => Promise<AgentDbMessageRecord[]>;
};

function getAgentDbApi(): AgentDbApi | null {
  const api = (window as any).tamux ?? (window as any).amux;
  if (!api) return null;
  return api as AgentDbApi;
}

function saveAgentSettings(s: AgentSettings) {
  scheduleJsonWrite(AGENT_SETTINGS_FILE, s);
}

function syncChatCounters(chat: AgentChatState) {
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

  _threadId = Math.max(_threadId, maxThread);
  _msgId = Math.max(_msgId, maxMessage);
}

function serializeThread(thread: AgentThread): AgentDbThreadRecord {
  return {
    id: thread.id,
    workspace_id: thread.workspaceId ?? null,
    surface_id: thread.surfaceId ?? null,
    pane_id: thread.paneId ?? null,
    agent_name: thread.agentName ?? null,
    title: thread.title,
    created_at: thread.createdAt,
    updated_at: thread.updatedAt,
    message_count: thread.messageCount,
    total_tokens: thread.totalTokens,
    last_preview: thread.lastMessagePreview,
  };
}

function serializeMessage(message: AgentMessage): AgentDbMessageRecord {
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
      reasoningTokens: message.reasoningTokens ?? null,
      audioTokens: message.audioTokens ?? null,
      videoTokens: message.videoTokens ?? null,
      cost: message.cost ?? null,
      tps: message.tps ?? null,
      isCompactionSummary: message.isCompactionSummary,
      isStreaming: message.isStreaming ?? false,
    }),
  };
}

function deserializeThread(thread: AgentDbThreadRecord): AgentThread {
  return {
    id: thread.id,
    workspaceId: thread.workspace_id,
    surfaceId: thread.surface_id,
    paneId: thread.pane_id,
    agentName: thread.agent_name ?? "assistant",
    title: thread.title,
    createdAt: thread.created_at,
    updatedAt: thread.updated_at,
    messageCount: thread.message_count,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: thread.total_tokens,
    compactionCount: 0,
    lastMessagePreview: thread.last_preview,
  };
}

function deserializeMessage(message: AgentDbMessageRecord): AgentMessage {
  const metadata = typeof message.metadata_json === "string"
    ? JSON.parse(message.metadata_json)
    : {};
  return {
    id: message.id,
    threadId: message.thread_id,
    createdAt: message.created_at,
    role: message.role as AgentRole,
    content: message.content,
    provider: message.provider ?? undefined,
    model: message.model ?? undefined,
    toolCalls: typeof message.tool_calls_json === "string" ? JSON.parse(message.tool_calls_json) : undefined,
    toolName: metadata.toolName ?? undefined,
    toolCallId: metadata.toolCallId ?? undefined,
    toolArguments: metadata.toolArguments ?? undefined,
    toolStatus: metadata.toolStatus ?? undefined,
    inputTokens: message.input_tokens ?? 0,
    outputTokens: message.output_tokens ?? 0,
    totalTokens: message.total_tokens ?? 0,
    reasoning: message.reasoning ?? undefined,
    reasoningTokens: metadata.reasoningTokens ?? undefined,
    audioTokens: metadata.audioTokens ?? undefined,
    videoTokens: metadata.videoTokens ?? undefined,
    cost: metadata.cost ?? undefined,
    tps: metadata.tps ?? undefined,
    isCompactionSummary: Boolean(metadata.isCompactionSummary),
    isStreaming: Boolean(metadata.isStreaming),
  };
}

export const useAgentStore = create<AgentState>((set, get) => ({
  threads: [],
  messages: {},
  activeThreadId: null,
  agentPanelOpen: false,
  agentSettings: loadAgentSettings(),
  searchQuery: "",

  createThread: (opts) => {
    const id = `thread_${++_threadId}`;
    const now = Date.now();
    const thread: AgentThread = {
      id,
      workspaceId: opts.workspaceId ?? null,
      surfaceId: opts.surfaceId ?? null,
      paneId: opts.paneId ?? null,
      agentName: get().agentSettings.agentName,
      title: opts.title ?? "New Conversation",
      createdAt: now,
      updatedAt: now,
      messageCount: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalTokens: 0,
      compactionCount: 0,
      lastMessagePreview: "",
    };
    set((s) => {
      const next: AgentChatState = {
        threads: [thread, ...s.threads],
        messages: { ...s.messages, [id]: [] },
        activeThreadId: id,
      };
      void getAgentDbApi()?.dbCreateThread?.(serializeThread(thread));
      return next;
    });
    return id;
  },

  deleteThread: (id) => {
    set((s) => {
      const { [id]: _, ...rest } = s.messages;
      const next: AgentChatState = {
        threads: s.threads.filter((t) => t.id !== id),
        messages: rest,
        activeThreadId: s.activeThreadId === id ? null : s.activeThreadId,
      };
      void getAgentDbApi()?.dbDeleteThread?.(id);
      return next;
    });
  },

  setActiveThread: (id) => set({ activeThreadId: id }),

  searchThreads: (query) => {
    const lower = query.toLowerCase();
    return get().threads.filter(
      (t) =>
        t.title.toLowerCase().includes(lower) ||
        t.lastMessagePreview.toLowerCase().includes(lower) ||
        t.agentName.toLowerCase().includes(lower)
    );
  },

  addMessage: (threadId, msg) => {
    const id = `msg_${++_msgId}`;
    const full: AgentMessage = {
      ...msg,
      id,
      threadId,
      createdAt: Date.now(),
    };
    set((s) => {
      const threadMsgs = [...(s.messages[threadId] ?? []), full];
      const _thread = s.threads.find((t) => t.id === threadId); void _thread;
      const next: AgentChatState = {
        messages: { ...s.messages, [threadId]: threadMsgs },
        threads: s.threads.map((t) =>
          t.id === threadId
            ? {
              ...t,
              messageCount: t.messageCount + 1,
              updatedAt: Date.now(),
              totalInputTokens: t.totalInputTokens + msg.inputTokens,
              totalOutputTokens: t.totalOutputTokens + msg.outputTokens,
              totalTokens: t.totalTokens + msg.totalTokens,
              lastMessagePreview: msg.content.slice(0, 100),
            }
            : t
        ),
        activeThreadId: s.activeThreadId,
      };
      const updatedThread = next.threads.find((thread) => thread.id === threadId);
      if (updatedThread) {
        void getAgentDbApi()?.dbCreateThread?.(serializeThread(updatedThread));
      }
      void getAgentDbApi()?.dbAddMessage?.(serializeMessage(full));
      return next;
    });
  },

  updateLastAssistantMessage: (threadId, content, streaming, meta) => {
    set((s) => {
      const msgs = s.messages[threadId];
      if (!msgs || msgs.length === 0) return s;
      const last = msgs[msgs.length - 1];
      if (last.role !== "assistant") return s;
      const nextInputTokens = meta?.inputTokens ?? last.inputTokens;
      const nextOutputTokens = meta?.outputTokens ?? last.outputTokens;
      const nextTotalTokens = meta?.totalTokens ?? last.totalTokens;
      const updatedLast: AgentMessage = {
        ...last,
        content,
        isStreaming: streaming ?? false,
        inputTokens: nextInputTokens,
        outputTokens: nextOutputTokens,
        totalTokens: nextTotalTokens,
        reasoning: meta?.reasoning ?? last.reasoning,
        reasoningTokens: meta?.reasoningTokens ?? last.reasoningTokens,
        audioTokens: meta?.audioTokens ?? last.audioTokens,
        videoTokens: meta?.videoTokens ?? last.videoTokens,
        cost: meta?.cost ?? last.cost,
        tps: meta?.tps ?? last.tps,
        toolCalls: meta?.toolCalls ?? last.toolCalls,
        provider: meta?.provider ?? last.provider,
        model: meta?.model ?? last.model,
      };
      const updated = [...msgs.slice(0, -1), updatedLast];
      const tokenDeltaIn = nextInputTokens - last.inputTokens;
      const tokenDeltaOut = nextOutputTokens - last.outputTokens;
      const tokenDeltaTotal = nextTotalTokens - last.totalTokens;
      const nextThreads = s.threads.map((thread) =>
        thread.id === threadId
          ? {
            ...thread,
            totalInputTokens: thread.totalInputTokens + tokenDeltaIn,
            totalOutputTokens: thread.totalOutputTokens + tokenDeltaOut,
            totalTokens: thread.totalTokens + tokenDeltaTotal,
            updatedAt: Date.now(),
            lastMessagePreview: content.slice(0, 100),
          }
          : thread,
      );
      const next = { messages: { ...s.messages, [threadId]: updated }, threads: nextThreads };
      const updatedThread = nextThreads.find((thread) => thread.id === threadId);
      if (updatedThread) {
        void getAgentDbApi()?.dbCreateThread?.(serializeThread(updatedThread));
      }
      void getAgentDbApi()?.dbAddMessage?.(serializeMessage(updatedLast));
      return next;
    });
  },

  getThreadMessages: (threadId) => get().messages[threadId] ?? [],

  toggleAgentPanel: () => set((s) => ({ agentPanelOpen: !s.agentPanelOpen })),
  setSearchQuery: (q) => set({ searchQuery: q }),

  updateAgentSetting: (key, value) => {
    set((s) => {
      const nextValue = key === "activeProvider"
        ? normalizeAgentProviderId(value)
        : value;
      const updated = { ...s.agentSettings, [key]: nextValue };
      saveAgentSettings(updated);
      return { agentSettings: updated };
    });
  },

  resetAgentSettings: () => {
    const def = { ...DEFAULT_AGENT_SETTINGS };
    saveAgentSettings(def);
    set({ agentSettings: def });
  },

  getThreadsForPane: (paneId) => get().threads.filter((t) => t.paneId === paneId),
}));

export async function hydrateAgentStore(): Promise<void> {
  const diskState = await readPersistedJson<AgentSettings>(AGENT_SETTINGS_FILE);
  if (diskState) {
    const merged: AgentSettings = {
      ...DEFAULT_AGENT_SETTINGS,
      ...diskState,
      activeProvider: normalizeAgentProviderId(diskState.activeProvider),
      agentBackend: normalizeAgentBackend(diskState.agentBackend),
      featherless: { ...DEFAULT_AGENT_SETTINGS.featherless, ...(diskState.featherless ?? {}) },
      openai: { ...DEFAULT_AGENT_SETTINGS.openai, ...(diskState.openai ?? {}) },
      anthropic: { ...DEFAULT_AGENT_SETTINGS.anthropic, ...(diskState.anthropic ?? {}) },
      qwen: { ...DEFAULT_AGENT_SETTINGS.qwen, ...(diskState.qwen ?? {}) },
      "qwen-deepinfra": { ...DEFAULT_AGENT_SETTINGS["qwen-deepinfra"], ...(diskState["qwen-deepinfra"] ?? {}) },
      kimi: { ...DEFAULT_AGENT_SETTINGS.kimi, ...(diskState.kimi ?? {}) },
      "z.ai": { ...DEFAULT_AGENT_SETTINGS["z.ai"], ...(diskState["z.ai"] ?? {}) },
      openrouter: { ...DEFAULT_AGENT_SETTINGS.openrouter, ...(diskState.openrouter ?? {}) },
      cerebras: { ...DEFAULT_AGENT_SETTINGS.cerebras, ...(diskState.cerebras ?? {}) },
      together: { ...DEFAULT_AGENT_SETTINGS.together, ...(diskState.together ?? {}) },
      groq: { ...DEFAULT_AGENT_SETTINGS.groq, ...(diskState.groq ?? {}) },
      ollama: { ...DEFAULT_AGENT_SETTINGS.ollama, ...(diskState.ollama ?? {}) },
      chutes: { ...DEFAULT_AGENT_SETTINGS.chutes, ...(diskState.chutes ?? {}) },
      huggingface: { ...DEFAULT_AGENT_SETTINGS.huggingface, ...(diskState.huggingface ?? {}) },
      minimax: { ...DEFAULT_AGENT_SETTINGS.minimax, ...(diskState.minimax ?? {}) },
      custom: { ...DEFAULT_AGENT_SETTINGS.custom, ...(diskState.custom ?? {}) },
    };
    useAgentStore.setState({ agentSettings: merged });
  }

  const api = getAgentDbApi();
  const dbThreads = await api?.dbListThreads?.();
  if (Array.isArray(dbThreads) && dbThreads.length > 0) {
    const messages: Record<string, AgentMessage[]> = {};
    for (const thread of dbThreads) {
      const threadMessages = await api?.dbListMessages?.(thread.id, 500) ?? [];
      messages[thread.id] = threadMessages.map(deserializeMessage);
    }

    const hydrated: AgentChatState = {
      threads: dbThreads.map((thread) => ({
        ...deserializeThread(thread),
        messageCount: messages[thread.id]?.length ?? thread.message_count,
        lastMessagePreview: messages[thread.id]?.[messages[thread.id].length - 1]?.content?.slice(0, 100) ?? thread.last_preview ?? "",
      })),
      messages,
      activeThreadId: null,
    };

    syncChatCounters(hydrated);
    useAgentStore.setState(hydrated);
    return;
  }

  const legacyChat = await readPersistedJson<AgentChatState>(AGENT_CHAT_FILE);
  if (!legacyChat || !Array.isArray(legacyChat.threads) || typeof legacyChat.messages !== "object") {
    return;
  }

  const hydrated: AgentChatState = {
    threads: legacyChat.threads,
    messages: legacyChat.messages,
    activeThreadId: legacyChat.activeThreadId ?? null,
  };
  syncChatCounters(hydrated);
  useAgentStore.setState(hydrated);

  for (const thread of hydrated.threads) {
    await api?.dbCreateThread?.(serializeThread(thread));
    for (const message of hydrated.messages[thread.id] ?? []) {
      await api?.dbAddMessage?.(serializeMessage(message));
    }
  }
}
