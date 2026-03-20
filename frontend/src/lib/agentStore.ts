import { create } from "zustand";
import type { WorkspaceId, SurfaceId, PaneId } from "./types";
import type { ToolCall } from "./agentTools";
import { readPersistedJson, scheduleJsonWrite } from "./persistence";

// ---------------------------------------------------------------------------
// Types matching amux-windows AgentConversationThread/Message
// ---------------------------------------------------------------------------
export interface AgentThread {
  id: string;
  daemonThreadId?: string | null;
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
  | "kimi-coding-plan"
  | "z.ai"
  | "z.ai-coding-plan"
  | "openrouter"
  | "cerebras"
  | "together"
  | "groq"
  | "ollama"
  | "chutes"
  | "huggingface"
  | "minimax"
  | "minimax-coding-plan"
  | "alibaba-coding-plan"
  | "opencode-zen"
  | "custom";

const AGENT_PROVIDER_IDS: AgentProviderId[] = [
  "featherless",
  "openai",
  "anthropic",
  "qwen",
  "qwen-deepinfra",
  "kimi",
  "kimi-coding-plan",
  "z.ai",
  "z.ai-coding-plan",
  "openrouter",
  "cerebras",
  "together",
  "groq",
  "ollama",
  "chutes",
  "huggingface",
  "minimax",
  "minimax-coding-plan",
  "alibaba-coding-plan",
  "opencode-zen",
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

export type ApiType = "openai" | "anthropic";
export type AuthMethod = "bearer" | "x-api-key";

export interface ModelDefinition {
  id: string;
  name: string;
  contextWindow: number;
}

export interface ProviderDefinition {
  id: AgentProviderId;
  name: string;
  defaultBaseUrl: string;
  defaultModel: string;
  apiType: ApiType;
  authMethod: AuthMethod;
  models: ModelDefinition[];
  supportsModelFetch: boolean;
  anthropicBaseUrl?: string;
}

const OPENAI_MODELS: ModelDefinition[] = [
  { id: "gpt-4o", name: "GPT-4o", contextWindow: 128000 },
  { id: "gpt-4o-mini", name: "GPT-4o Mini", contextWindow: 128000 },
  { id: "gpt-4-turbo", name: "GPT-4 Turbo", contextWindow: 128000 },
  { id: "o1", name: "o1", contextWindow: 200000 },
  { id: "o1-mini", name: "o1 Mini", contextWindow: 128000 },
];

const ANTHROPIC_MODELS: ModelDefinition[] = [
  { id: "claude-opus-4-20250514", name: "Claude Opus 4", contextWindow: 200000 },
  { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4", contextWindow: 200000 },
  { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet", contextWindow: 200000 },
  { id: "claude-3-5-haiku-20241022", name: "Claude 3.5 Haiku", contextWindow: 200000 },
];

const ZAI_MODELS: ModelDefinition[] = [
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "glm-4-plus", name: "GLM-4 Plus", contextWindow: 128000 },
  { id: "glm-4", name: "GLM-4", contextWindow: 128000 },
];

const KIMI_MODELS: ModelDefinition[] = [
  { id: "moonshot-v1-8k", name: "Moonshot V1 8K", contextWindow: 8192 },
  { id: "moonshot-v1-32k", name: "Moonshot V1 32K", contextWindow: 32768 },
  { id: "moonshot-v1-128k", name: "Moonshot V1 128K", contextWindow: 131072 },
];

const KIMI_CODING_MODELS: ModelDefinition[] = [
  { id: "kimi-for-coding", name: "Kimi for Coding", contextWindow: 262144 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
];

const MINIMAX_MODELS: ModelDefinition[] = [
  { id: "MiniMax-M2.7", name: "MiniMax M2.7", contextWindow: 205000 },
  { id: "MiniMax-M2.5", name: "MiniMax M2.5", contextWindow: 205000 },
  { id: "MiniMax-M1-80k", name: "MiniMax M1 80K", contextWindow: 80000 },
];

const ALIBABA_CODING_MODELS: ModelDefinition[] = [
  { id: "qwen3-coder", name: "Qwen3 Coder", contextWindow: 128000 },
  { id: "qwen3-coder-next", name: "Qwen3 Coder Next", contextWindow: 128000 },
  { id: "qwen3.5-plus", name: "Qwen3.5 Plus", contextWindow: 128000 },
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
  { id: "MiniMax-M2.5", name: "MiniMax M2.5", contextWindow: 205000 },
];

const OPENCODE_ZEN_MODELS: ModelDefinition[] = [
  { id: "claude-opus-4-6", name: "Claude Opus 4.6", contextWindow: 200000 },
  { id: "claude-sonnet-4-5", name: "Claude Sonnet 4.5", contextWindow: 200000 },
  { id: "claude-sonnet-4", name: "Claude Sonnet 4", contextWindow: 200000 },
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 128000 },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 128000 },
  { id: "minimax-m2.5", name: "MiniMax M2.5", contextWindow: 205000 },
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
];

const GROQ_MODELS: ModelDefinition[] = [
  { id: "llama-3.3-70b-versatile", name: "Llama 3.3 70B Versatile", contextWindow: 128000 },
  { id: "llama-3.1-8b-instant", name: "Llama 3.1 8B", contextWindow: 128000 },
];

const OLLAMA_MODELS: ModelDefinition[] = [
  { id: "llama3.1", name: "Llama 3.1", contextWindow: 128000 },
  { id: "llama3.2", name: "Llama 3.2", contextWindow: 128000 },
  { id: "qwen2.5", name: "Qwen 2.5", contextWindow: 128000 },
  { id: "codellama", name: "Code Llama", contextWindow: 16384 },
];

const EMPTY_MODELS: ModelDefinition[] = [];

export const PROVIDER_DEFINITIONS: ProviderDefinition[] = [
  { id: "featherless", name: "Featherless", defaultBaseUrl: "https://api.featherless.ai/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false },
  { id: "openai", name: "OpenAI", defaultBaseUrl: "https://api.openai.com/v1", defaultModel: "gpt-4o", apiType: "openai", authMethod: "bearer", models: OPENAI_MODELS, supportsModelFetch: true },
  { id: "anthropic", name: "Anthropic", defaultBaseUrl: "https://api.anthropic.com", defaultModel: "claude-sonnet-4-20250514", apiType: "anthropic", authMethod: "x-api-key", models: ANTHROPIC_MODELS, supportsModelFetch: false },
  { id: "qwen", name: "Qwen", defaultBaseUrl: "https://api.qwen.com/v1", defaultModel: "qwen-max", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true },
  { id: "qwen-deepinfra", name: "Qwen (DeepInfra)", defaultBaseUrl: "https://api.deepinfra.com/v1/openai", defaultModel: "Qwen/Qwen2.5-72B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true },
  { id: "kimi", name: "Kimi (Moonshot)", defaultBaseUrl: "https://api.moonshot.ai/v1", defaultModel: "moonshot-v1-32k", apiType: "openai", authMethod: "bearer", models: KIMI_MODELS, supportsModelFetch: true },
  { id: "kimi-coding-plan", name: "Kimi Coding Plan", defaultBaseUrl: "https://api.kimi.com/coding/v1", defaultModel: "kimi-for-coding", apiType: "openai", authMethod: "bearer", models: KIMI_CODING_MODELS, supportsModelFetch: false },
  { id: "z.ai", name: "Z.AI (GLM)", defaultBaseUrl: "https://api.z.ai/api/paas/v4", defaultModel: "glm-4-plus", apiType: "openai", authMethod: "bearer", models: ZAI_MODELS, supportsModelFetch: false },
  { id: "z.ai-coding-plan", name: "Z.AI Coding Plan", defaultBaseUrl: "https://api.z.ai/api/coding/paas/v4", defaultModel: "glm-5", apiType: "openai", authMethod: "bearer", models: ZAI_MODELS, supportsModelFetch: false },
  { id: "openrouter", name: "OpenRouter", defaultBaseUrl: "https://openrouter.ai/api/v1", defaultModel: "anthropic/claude-sonnet-4", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true },
  { id: "cerebras", name: "Cerebras", defaultBaseUrl: "https://api.cerebras.ai/v1", defaultModel: "llama-3.3-70b", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true },
  { id: "together", name: "Together", defaultBaseUrl: "https://api.together.xyz/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct-Turbo", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true },
  { id: "groq", name: "Groq", defaultBaseUrl: "https://api.groq.com/openai/v1", defaultModel: "llama-3.3-70b-versatile", apiType: "openai", authMethod: "bearer", models: GROQ_MODELS, supportsModelFetch: true },
  { id: "ollama", name: "Ollama", defaultBaseUrl: "http://localhost:11434/v1", defaultModel: "llama3.1", apiType: "openai", authMethod: "bearer", models: OLLAMA_MODELS, supportsModelFetch: true },
  { id: "chutes", name: "Chutes", defaultBaseUrl: "https://llm.chutes.ai/v1", defaultModel: "deepseek-ai/DeepSeek-V3", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false },
  { id: "huggingface", name: "Hugging Face", defaultBaseUrl: "https://api-inference.huggingface.co/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false },
  { id: "minimax", name: "MiniMax", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M1-80k", apiType: "anthropic", authMethod: "bearer", models: MINIMAX_MODELS, supportsModelFetch: false },
  { id: "minimax-coding-plan", name: "MiniMax Coding Plan", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M2.7", apiType: "anthropic", authMethod: "bearer", models: MINIMAX_MODELS, supportsModelFetch: false },
  { id: "alibaba-coding-plan", name: "Alibaba Coding Plan", defaultBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/v1", defaultModel: "qwen3-coder", apiType: "openai", authMethod: "bearer", models: ALIBABA_CODING_MODELS, supportsModelFetch: true, anthropicBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic" },
  { id: "opencode-zen", name: "OpenCode Zen", defaultBaseUrl: "https://opencode.ai/zen/v1", defaultModel: "claude-sonnet-4-5", apiType: "anthropic", authMethod: "bearer", models: OPENCODE_ZEN_MODELS, supportsModelFetch: true },
  { id: "custom", name: "Custom", defaultBaseUrl: "", defaultModel: "", apiType: "openai", authMethod: "bearer", models: EMPTY_MODELS, supportsModelFetch: false },
];

export function getProviderDefinition(id: AgentProviderId): ProviderDefinition | undefined {
  return PROVIDER_DEFINITIONS.find((p) => p.id === id);
}

export function getProviderApiType(providerId: AgentProviderId, model: string): ApiType {
  const def = getProviderDefinition(providerId);
  if (!def) return "openai";
  
  if (def.anthropicBaseUrl && model.startsWith("claude")) {
    return "anthropic";
  }
  if (providerId === "opencode-zen" && !model.startsWith("claude")) {
    return "openai";
  }
  return def.apiType;
}

export function getProviderBaseUrl(providerId: AgentProviderId, model: string, configuredUrl: string): string {
  if (configuredUrl) return configuredUrl;
  
  const def = getProviderDefinition(providerId);
  if (!def) return configuredUrl;
  
  if (def.anthropicBaseUrl && model.startsWith("claude")) {
    return def.anthropicBaseUrl;
  }
  return def.defaultBaseUrl;
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

export type AgentTodoStatus = "pending" | "in_progress" | "completed" | "blocked";

export interface AgentTodoItem {
  id: string;
  content: string;
  status: AgentTodoStatus;
  position: number;
  stepIndex?: number | null;
  createdAt?: number | null;
  updatedAt?: number | null;
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
  "kimi-coding-plan": AgentProviderConfig;
  "z.ai": AgentProviderConfig;
  "z.ai-coding-plan": AgentProviderConfig;
  openrouter: AgentProviderConfig;
  cerebras: AgentProviderConfig;
  together: AgentProviderConfig;
  groq: AgentProviderConfig;
  ollama: AgentProviderConfig;
  chutes: AgentProviderConfig;
  huggingface: AgentProviderConfig;
  minimax: AgentProviderConfig;
  "minimax-coding-plan": AgentProviderConfig;
  "alibaba-coding-plan": AgentProviderConfig;
  "opencode-zen": AgentProviderConfig;
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

  reasoningEffort: "none" | "minimal" | "low" | "medium" | "high" | "xhigh";

  autoCompactContext: boolean;
  maxContextMessages: number;
  maxToolLoops: number;
  maxRetries: number;
  retryDelayMs: number;
  contextWindowTokens: number;
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
  "kimi-coding-plan": { baseUrl: "https://api.kimi.com/coding/v1", model: "kimi-for-coding", apiKey: "" },
  "z.ai": { baseUrl: "https://api.z.ai/api/paas/v4", model: "glm-4-plus", apiKey: "" },
  "z.ai-coding-plan": { baseUrl: "https://api.z.ai/api/coding/paas/v4", model: "glm-5", apiKey: "" },
  openrouter: { baseUrl: "https://openrouter.ai/api/v1", model: "anthropic/claude-sonnet-4", apiKey: "" },
  cerebras: { baseUrl: "https://api.cerebras.ai/v1", model: "llama-3.3-70b", apiKey: "" },
  together: { baseUrl: "https://api.together.xyz/v1", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", apiKey: "" },
  groq: { baseUrl: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", apiKey: "" },
  ollama: { baseUrl: "http://localhost:11434/v1", model: "llama3.1", apiKey: "" },
  chutes: { baseUrl: "https://llm.chutes.ai/v1", model: "deepseek-ai/DeepSeek-V3", apiKey: "" },
  huggingface: { baseUrl: "https://api-inference.huggingface.co/v1", model: "meta-llama/Llama-3.3-70B-Instruct", apiKey: "" },
  minimax: { baseUrl: "https://api.minimax.io/anthropic", model: "MiniMax-M1-80k", apiKey: "" },
  "minimax-coding-plan": { baseUrl: "https://api.minimax.io/anthropic", model: "MiniMax-M2.7", apiKey: "" },
  "alibaba-coding-plan": { baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen3-coder", apiKey: "" },
  "opencode-zen": { baseUrl: "https://opencode.ai/zen/v1", model: "claude-sonnet-4-5", apiKey: "" },
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

  reasoningEffort: "high",

  autoCompactContext: true,
  maxContextMessages: 100,
  maxToolLoops: 0,
  maxRetries: 3,
  retryDelayMs: 2000,
  contextWindowTokens: 128000,
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
  todos: Record<string, AgentTodoItem[]>;
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
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  getThreadTodos: (threadId: string) => AgentTodoItem[];
  setThreadDaemonId: (threadId: string, daemonThreadId: string | null) => void;

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
const AGENT_DAEMON_THREAD_MAP_FILE = "agent-daemon-thread-map.json";
const AGENT_ACTIVE_THREAD_FILE = "agent-active-thread.json";

type AgentChatState = {
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

function persistDaemonThreadMap(threads: AgentThread[]) {
  const mapping = Object.fromEntries(
    threads
      .filter((thread) => typeof thread.daemonThreadId === "string" && thread.daemonThreadId)
      .map((thread) => [thread.id, thread.daemonThreadId]),
  );
  scheduleJsonWrite(AGENT_DAEMON_THREAD_MAP_FILE, mapping);
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
    daemonThreadId: null,
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
  let metadata: Record<string, unknown> = {};
  try {
    metadata = typeof message.metadata_json === "string"
      ? JSON.parse(message.metadata_json)
      : {};
  } catch {
    metadata = {};
  }
  let toolCalls: ToolCall[] | undefined;
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
    toolCalls,
    toolName: (metadata.toolName as string) ?? undefined,
    toolCallId: (metadata.toolCallId as string) ?? undefined,
    toolArguments: (metadata.toolArguments as string) ?? undefined,
    toolStatus: (metadata.toolStatus as AgentMessage["toolStatus"]) ?? undefined,
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
    isStreaming: Boolean(metadata.isStreaming),
  };
}

export const useAgentStore = create<AgentState>((set, get) => ({
  threads: [],
  messages: {},
  todos: {},
  activeThreadId: null,
  agentPanelOpen: false,
  agentSettings: loadAgentSettings(),
  searchQuery: "",

  createThread: (opts) => {
    const id = `thread_${++_threadId}`;
    const now = Date.now();
    const thread: AgentThread = {
      id,
      daemonThreadId: null,
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
        todos: { ...s.todos, [id]: [] },
        activeThreadId: id,
      };
      persistDaemonThreadMap(next.threads);
      scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
      void getAgentDbApi()?.dbCreateThread?.(serializeThread(thread));
      return next;
    });
    return id;
  },

  deleteThread: (id) => {
    set((s) => {
      const { [id]: _, ...rest } = s.messages;
      const { [id]: __, ...todoRest } = s.todos;
      const next: AgentChatState = {
        threads: s.threads.filter((t) => t.id !== id),
        messages: rest,
        todos: todoRest,
        activeThreadId: s.activeThreadId === id ? null : s.activeThreadId,
      };
      persistDaemonThreadMap(next.threads);
      void getAgentDbApi()?.dbDeleteThread?.(id);
      return next;
    });
  },

  setActiveThread: (id) => {
    set({ activeThreadId: id });
    scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
  },

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
        todos: s.todos,
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
      void (async () => {
        const api = getAgentDbApi();
        if (updatedThread) await api?.dbCreateThread?.(serializeThread(updatedThread));
        await api?.dbAddMessage?.(serializeMessage(full));
      })();
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
      void (async () => {
        const api = getAgentDbApi();
        if (updatedThread) await api?.dbCreateThread?.(serializeThread(updatedThread));
        await api?.dbAddMessage?.(serializeMessage(updatedLast));
      })();
      return next;
    });
  },

  getThreadMessages: (threadId) => get().messages[threadId] ?? [],
  setThreadTodos: (threadId, todos) => {
    set((s) => ({
      todos: { ...s.todos, [threadId]: [...todos].sort((a, b) => a.position - b.position) },
    }));
  },
  getThreadTodos: (threadId) => get().todos[threadId] ?? [],
  setThreadDaemonId: (threadId, daemonThreadId) => {
    set((s) => {
      const threads = s.threads.map((thread) =>
        thread.id === threadId ? { ...thread, daemonThreadId } : thread,
      );
      persistDaemonThreadMap(threads);
      return { threads };
    });
  },

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
      "kimi-coding-plan": { ...DEFAULT_AGENT_SETTINGS["kimi-coding-plan"], ...(diskState["kimi-coding-plan"] ?? {}) },
      "z.ai": { ...DEFAULT_AGENT_SETTINGS["z.ai"], ...(diskState["z.ai"] ?? {}) },
      "z.ai-coding-plan": { ...DEFAULT_AGENT_SETTINGS["z.ai-coding-plan"], ...(diskState["z.ai-coding-plan"] ?? {}) },
      openrouter: { ...DEFAULT_AGENT_SETTINGS.openrouter, ...(diskState.openrouter ?? {}) },
      cerebras: { ...DEFAULT_AGENT_SETTINGS.cerebras, ...(diskState.cerebras ?? {}) },
      together: { ...DEFAULT_AGENT_SETTINGS.together, ...(diskState.together ?? {}) },
      groq: { ...DEFAULT_AGENT_SETTINGS.groq, ...(diskState.groq ?? {}) },
      ollama: { ...DEFAULT_AGENT_SETTINGS.ollama, ...(diskState.ollama ?? {}) },
      chutes: { ...DEFAULT_AGENT_SETTINGS.chutes, ...(diskState.chutes ?? {}) },
      huggingface: { ...DEFAULT_AGENT_SETTINGS.huggingface, ...(diskState.huggingface ?? {}) },
      minimax: { ...DEFAULT_AGENT_SETTINGS.minimax, ...(diskState.minimax ?? {}) },
      "minimax-coding-plan": { ...DEFAULT_AGENT_SETTINGS["minimax-coding-plan"], ...(diskState["minimax-coding-plan"] ?? {}) },
      "alibaba-coding-plan": { ...DEFAULT_AGENT_SETTINGS["alibaba-coding-plan"], ...(diskState["alibaba-coding-plan"] ?? {}) },
      "opencode-zen": { ...DEFAULT_AGENT_SETTINGS["opencode-zen"], ...(diskState["opencode-zen"] ?? {}) },
      custom: { ...DEFAULT_AGENT_SETTINGS.custom, ...(diskState.custom ?? {}) },
    };
    useAgentStore.setState({ agentSettings: merged });
  }

  const api = getAgentDbApi();
  const daemonThreadMap = await readPersistedJson<Record<string, string>>(AGENT_DAEMON_THREAD_MAP_FILE) ?? {};
  const savedActiveThread = await readPersistedJson<{ activeThreadId: string | null }>(AGENT_ACTIVE_THREAD_FILE);
  const dbThreads = await api?.dbListThreads?.();
  if (Array.isArray(dbThreads) && dbThreads.length > 0) {
    const messages: Record<string, AgentMessage[]> = {};
    for (const thread of dbThreads) {
      const threadMessages = await api?.dbListMessages?.(thread.id, 500) ?? [];
      messages[thread.id] = threadMessages.map(deserializeMessage);
    }

    const hydratedThreads = dbThreads.map((thread) => ({
      ...deserializeThread(thread),
      daemonThreadId: daemonThreadMap[thread.id] ?? null,
      messageCount: messages[thread.id]?.length ?? thread.message_count,
      lastMessagePreview: messages[thread.id]?.[messages[thread.id].length - 1]?.content?.slice(0, 100) ?? thread.last_preview ?? "",
    }));
    // Restore the persisted active thread, falling back to the most recent
    const savedId = savedActiveThread?.activeThreadId;
    const restoredId = (savedId && hydratedThreads.some((t) => t.id === savedId))
      ? savedId
      : (hydratedThreads.length > 0 ? hydratedThreads.reduce((a, b) => (a.updatedAt >= b.updatedAt ? a : b)).id : null);
    const hydrated: AgentChatState = {
      threads: hydratedThreads,
      messages,
      todos: {},
      activeThreadId: restoredId,
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
    threads: legacyChat.threads.map((thread) => ({
      ...thread,
      daemonThreadId: daemonThreadMap[thread.id] ?? thread.daemonThreadId ?? null,
    })),
    messages: legacyChat.messages,
    todos: {},
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
