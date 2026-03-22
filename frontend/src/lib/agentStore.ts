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
  upstreamThreadId?: string | null;
  upstreamTransport?: ApiTransportMode;
  upstreamProvider?: AgentProviderId | null;
  upstreamModel?: string | null;
  upstreamAssistantId?: string | null;
}

export type AgentRole = "user" | "assistant" | "system" | "tool";

export type AgentProviderId =
  | "featherless"
  | "openai"
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

export const AGENT_PROVIDER_IDS: AgentProviderId[] = [
  "featherless",
  "openai",
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
  customModelName: string;
  apiKey: string;
  assistantId: string;
  apiTransport: ApiTransportMode;
  authSource: AuthSource;
  customContextWindowTokens: number | null;
}

export interface ProviderAuthState {
  provider_id: string;
  provider_name: string;
  authenticated: boolean;
  auth_source: AuthSource;
  model: string;
  base_url: string;
}

export interface SubAgentDefinition {
  id: string;
  name: string;
  provider: string;
  model: string;
  role?: string;
  system_prompt?: string;
  tool_whitelist?: string[];
  tool_blacklist?: string[];
  context_budget_tokens?: number;
  max_duration_secs?: number;
  supervisor_config?: {
    check_interval_secs?: number;
    stuck_timeout_secs?: number;
    max_retries?: number;
    intervention_level?: string;
  };
  enabled: boolean;
  created_at: number;
}

export type ApiType = "openai" | "anthropic";
export type AuthMethod = "bearer" | "x-api-key";
export type AuthSource = "api_key" | "chatgpt_subscription";
export type ApiTransportMode = "native_assistant" | "responses" | "chat_completions";
export type NativeTransportKind = "alibaba_assistant_api";

const API_KEY_ONLY_AUTH_SOURCES: AuthSource[] = ["api_key"];
const OPENAI_AUTH_SOURCES: AuthSource[] = ["chatgpt_subscription", "api_key"];

function normalizeApiTransport(
  providerId: AgentProviderId,
  value: unknown,
): ApiTransportMode {
  const normalized = value === "native_assistant"
    ? "native_assistant"
    : value === "chat_completions"
      ? "chat_completions"
      : "responses";
  return getSupportedApiTransports(providerId).includes(normalized)
    ? normalized
    : getDefaultApiTransport(providerId);
}

function normalizeAuthSource(
  providerId: AgentProviderId,
  value: unknown,
): AuthSource {
  const normalized = value === "chatgpt_subscription"
    ? "chatgpt_subscription"
    : "api_key";
  return getSupportedAuthSources(providerId).includes(normalized)
    ? normalized
    : getDefaultAuthSource(providerId);
}

function normalizeProviderConfig(
  providerId: AgentProviderId,
  fallback: AgentProviderConfig,
  value: Partial<AgentProviderConfig> | undefined,
): AgentProviderConfig {
  const authSource = normalizeAuthSource(providerId, value?.authSource ?? fallback.authSource);
  const requestedModel = typeof value?.model === "string" ? value.model.trim() : fallback.model.trim();
  const supportedModels = getProviderModels(providerId, authSource);
  const matchesKnownModel = requestedModel && supportedModels.some((entry) => entry.id === requestedModel);
  const model = requestedModel
    ? requestedModel
    : getDefaultModelForProvider(providerId, authSource);
  const customModelName = typeof value?.customModelName === "string"
    ? value.customModelName.trim()
    : "";
  return {
    ...fallback,
    ...(value ?? {}),
    model,
    customModelName: matchesKnownModel ? "" : (customModelName || (requestedModel && !matchesKnownModel ? requestedModel : "")),
    assistantId: typeof value?.assistantId === "string" ? value.assistantId : fallback.assistantId,
    apiTransport: normalizeApiTransport(providerId, value?.apiTransport ?? fallback.apiTransport),
    authSource,
    customContextWindowTokens:
      typeof value?.customContextWindowTokens === "number" && Number.isFinite(value.customContextWindowTokens)
        ? Math.max(1000, Math.trunc(value.customContextWindowTokens))
        : null,
  };
}

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
  supportedTransports: ApiTransportMode[];
  defaultTransport: ApiTransportMode;
  supportedAuthSources: AuthSource[];
  defaultAuthSource: AuthSource;
  nativeTransportKind?: NativeTransportKind;
  nativeBaseUrl?: string;
  supportsResponseContinuity: boolean;
}

const OPENAI_API_MODELS: ModelDefinition[] = [
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 1_000_000 },
  { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", contextWindow: 400_000 },
  { id: "gpt-5.4-nano", name: "GPT-5.4 Nano", contextWindow: 400_000 },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 400_000 },
  { id: "gpt-5.2-codex", name: "GPT-5.2 Codex", contextWindow: 400_000 },
  { id: "gpt-5.2", name: "GPT-5.2", contextWindow: 400_000 },
  { id: "gpt-5.1-codex-max", name: "GPT-5.1 Codex Max", contextWindow: 400_000 },
  { id: "gpt-5.1-codex", name: "GPT-5.1 Codex", contextWindow: 400_000 },
  { id: "gpt-5.1-codex-mini", name: "GPT-5.1 Codex Mini", contextWindow: 400_000 },
  { id: "gpt-5.1", name: "GPT-5.1", contextWindow: 400_000 },
  { id: "gpt-5-codex", name: "GPT-5 Codex", contextWindow: 400_000 },
  { id: "gpt-5-codex-mini", name: "GPT-5 Codex Mini", contextWindow: 200_000 },
  { id: "gpt-5", name: "GPT-5", contextWindow: 400_000 },
  { id: "codex-mini-latest", name: "Codex Mini Latest", contextWindow: 200_000 },
  { id: "o3", name: "o3", contextWindow: 200_000 },
  { id: "o4-mini", name: "o4 Mini", contextWindow: 200_000 },
];

const OPENAI_CHATGPT_SUBSCRIPTION_MODELS: ModelDefinition[] = [
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 1_000_000 },
  { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", contextWindow: 400_000 },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 400_000 },
  { id: "gpt-5.2-codex", name: "GPT-5.2 Codex", contextWindow: 400_000 },
  { id: "gpt-5.2", name: "GPT-5.2", contextWindow: 400_000 },
  { id: "gpt-5.1-codex-max", name: "GPT-5.1 Codex Max", contextWindow: 400_000 },
  { id: "gpt-5.1-codex-mini", name: "GPT-5.1 Codex Mini", contextWindow: 400_000 },
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
const CHAT_ONLY_TRANSPORTS: ApiTransportMode[] = ["chat_completions"];
const RESPONSES_AND_CHAT_TRANSPORTS: ApiTransportMode[] = ["responses", "chat_completions"];
const NATIVE_AND_CHAT_TRANSPORTS: ApiTransportMode[] = ["native_assistant", "chat_completions"];

export const PROVIDER_DEFINITIONS: ProviderDefinition[] = [
  { id: "featherless", name: "Featherless", defaultBaseUrl: "https://api.featherless.ai/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "openai", name: "OpenAI / ChatGPT", defaultBaseUrl: "https://api.openai.com/v1", defaultModel: "gpt-5.4", apiType: "openai", authMethod: "bearer", models: OPENAI_API_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: OPENAI_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
  { id: "qwen", name: "Qwen", defaultBaseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", defaultModel: "qwen-max", apiType: "openai", authMethod: "bearer", models: ALIBABA_CODING_MODELS, supportsModelFetch: true, supportedTransports: NATIVE_AND_CHAT_TRANSPORTS, defaultTransport: "native_assistant", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", nativeTransportKind: "alibaba_assistant_api", nativeBaseUrl: "https://dashscope-intl.aliyuncs.com/api/v1", supportsResponseContinuity: false },
  { id: "qwen-deepinfra", name: "Qwen (DeepInfra)", defaultBaseUrl: "https://api.deepinfra.com/v1/openai", defaultModel: "Qwen/Qwen2.5-72B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "kimi", name: "Kimi (Moonshot)", defaultBaseUrl: "https://api.moonshot.ai/v1", defaultModel: "moonshot-v1-32k", apiType: "openai", authMethod: "bearer", models: KIMI_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "kimi-coding-plan", name: "Kimi Coding Plan", defaultBaseUrl: "https://api.kimi.com/coding/v1", defaultModel: "kimi-for-coding", apiType: "openai", authMethod: "bearer", models: KIMI_CODING_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "z.ai", name: "Z.AI (GLM)", defaultBaseUrl: "https://api.z.ai/api/paas/v4", defaultModel: "glm-4-plus", apiType: "openai", authMethod: "bearer", models: ZAI_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "z.ai-coding-plan", name: "Z.AI Coding Plan", defaultBaseUrl: "https://api.z.ai/api/coding/paas/v4", defaultModel: "glm-5", apiType: "openai", authMethod: "bearer", models: ZAI_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "openrouter", name: "OpenRouter", defaultBaseUrl: "https://openrouter.ai/api/v1", defaultModel: "anthropic/claude-sonnet-4", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "cerebras", name: "Cerebras", defaultBaseUrl: "https://api.cerebras.ai/v1", defaultModel: "llama-3.3-70b", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "together", name: "Together", defaultBaseUrl: "https://api.together.xyz/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct-Turbo", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "groq", name: "Groq", defaultBaseUrl: "https://api.groq.com/openai/v1", defaultModel: "llama-3.3-70b-versatile", apiType: "openai", authMethod: "bearer", models: GROQ_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "ollama", name: "Ollama", defaultBaseUrl: "http://localhost:11434/v1", defaultModel: "llama3.1", apiType: "openai", authMethod: "bearer", models: OLLAMA_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "chutes", name: "Chutes", defaultBaseUrl: "https://llm.chutes.ai/v1", defaultModel: "deepseek-ai/DeepSeek-V3", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "huggingface", name: "Hugging Face", defaultBaseUrl: "https://api-inference.huggingface.co/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "minimax", name: "MiniMax", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M1-80k", apiType: "anthropic", authMethod: "bearer", models: MINIMAX_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "minimax-coding-plan", name: "MiniMax Coding Plan", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M2.7", apiType: "anthropic", authMethod: "bearer", models: MINIMAX_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "alibaba-coding-plan", name: "Alibaba Coding Plan", defaultBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/v1", defaultModel: "qwen3-coder", apiType: "openai", authMethod: "bearer", models: ALIBABA_CODING_MODELS, supportsModelFetch: true, anthropicBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic", supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "opencode-zen", name: "OpenCode Zen", defaultBaseUrl: "https://opencode.ai/zen/v1", defaultModel: "claude-sonnet-4-5", apiType: "anthropic", authMethod: "bearer", models: OPENCODE_ZEN_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "custom", name: "Custom", defaultBaseUrl: "", defaultModel: "", apiType: "openai", authMethod: "bearer", models: EMPTY_MODELS, supportsModelFetch: false, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
];

export function getProviderDefinition(id: AgentProviderId): ProviderDefinition | undefined {
  return PROVIDER_DEFINITIONS.find((p) => p.id === id);
}

export function getSupportedApiTransports(providerId: AgentProviderId): ApiTransportMode[] {
  return getProviderDefinition(providerId)?.supportedTransports ?? CHAT_ONLY_TRANSPORTS;
}

export function getDefaultApiTransport(providerId: AgentProviderId): ApiTransportMode {
  return getProviderDefinition(providerId)?.defaultTransport ?? "chat_completions";
}

export function getSupportedAuthSources(providerId: AgentProviderId): AuthSource[] {
  return getProviderDefinition(providerId)?.supportedAuthSources ?? API_KEY_ONLY_AUTH_SOURCES;
}

export function getDefaultAuthSource(providerId: AgentProviderId): AuthSource {
  return getProviderDefinition(providerId)?.defaultAuthSource ?? "api_key";
}

export function getProviderModels(
  providerId: AgentProviderId,
  authSource?: AuthSource,
): ModelDefinition[] {
  if (providerId === "openai" && authSource === "chatgpt_subscription") {
    return OPENAI_CHATGPT_SUBSCRIPTION_MODELS;
  }
  return getProviderDefinition(providerId)?.models ?? [];
}

export function getDefaultModelForProvider(
  providerId: AgentProviderId,
  authSource?: AuthSource,
): string {
  const models = getProviderModels(providerId, authSource);
  if (models.length > 0) {
    return models[0].id;
  }
  return getProviderDefinition(providerId)?.defaultModel ?? "";
}

export function getModelDefinition(
  providerId: AgentProviderId,
  modelId: string,
  authSource?: AuthSource,
): ModelDefinition | undefined {
  const trimmed = modelId.trim();
  if (!trimmed) return undefined;
  return getProviderModels(providerId, authSource).find((model) => model.id === trimmed);
}

export function getEffectiveContextWindow(
  providerId: AgentProviderId,
  config: Pick<AgentProviderConfig, "model" | "customContextWindowTokens" | "authSource">,
): number {
  if (providerId === "custom") {
    if (typeof config.customContextWindowTokens === "number" && config.customContextWindowTokens > 0) {
      return Math.max(1000, Math.trunc(config.customContextWindowTokens));
    }
    return 128_000;
  }

  return getModelDefinition(providerId, config.model, config.authSource)?.contextWindow ?? 128_000;
}

export function providerSupportsResponseContinuity(providerId: AgentProviderId): boolean {
  return Boolean(getProviderDefinition(providerId)?.supportsResponseContinuity);
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
  apiTransport?: ApiTransportMode;
  responseId?: string;
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
  featherless: { baseUrl: "https://api.featherless.ai/v1", model: "meta-llama/Llama-3.3-70B-Instruct", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  openai: { baseUrl: "https://api.openai.com/v1", model: "gpt-5.4", customModelName: "", apiKey: "", assistantId: "", apiTransport: "responses", authSource: "api_key", customContextWindowTokens: null },
  qwen: { baseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-max", customModelName: "", apiKey: "", assistantId: "", apiTransport: "native_assistant", authSource: "api_key", customContextWindowTokens: null },
  "qwen-deepinfra": { baseUrl: "https://api.deepinfra.com/v1/openai", model: "Qwen/Qwen2.5-72B-Instruct", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  kimi: { baseUrl: "https://api.moonshot.ai/v1", model: "moonshot-v1-32k", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "kimi-coding-plan": { baseUrl: "https://api.kimi.com/coding/v1", model: "kimi-for-coding", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "z.ai": { baseUrl: "https://api.z.ai/api/paas/v4", model: "glm-4-plus", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "z.ai-coding-plan": { baseUrl: "https://api.z.ai/api/coding/paas/v4", model: "glm-5", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  openrouter: { baseUrl: "https://openrouter.ai/api/v1", model: "anthropic/claude-sonnet-4", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  cerebras: { baseUrl: "https://api.cerebras.ai/v1", model: "llama-3.3-70b", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  together: { baseUrl: "https://api.together.xyz/v1", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  groq: { baseUrl: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", customModelName: "", apiKey: "", assistantId: "", apiTransport: "responses", authSource: "api_key", customContextWindowTokens: null },
  ollama: { baseUrl: "http://localhost:11434/v1", model: "llama3.1", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  chutes: { baseUrl: "https://llm.chutes.ai/v1", model: "deepseek-ai/DeepSeek-V3", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  huggingface: { baseUrl: "https://api-inference.huggingface.co/v1", model: "meta-llama/Llama-3.3-70B-Instruct", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  minimax: { baseUrl: "https://api.minimax.io/anthropic", model: "MiniMax-M1-80k", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "minimax-coding-plan": { baseUrl: "https://api.minimax.io/anthropic", model: "MiniMax-M2.7", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "alibaba-coding-plan": { baseUrl: "https://coding-intl.dashscope.aliyuncs.com/v1", model: "qwen3-coder", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  "opencode-zen": { baseUrl: "https://opencode.ai/zen/v1", model: "claude-sonnet-4-5", customModelName: "", apiKey: "", assistantId: "", apiTransport: "chat_completions", authSource: "api_key", customContextWindowTokens: null },
  custom: { baseUrl: "", model: "", customModelName: "", apiKey: "", assistantId: "", apiTransport: "responses", authSource: "api_key", customContextWindowTokens: 128_000 },

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
    meta?: Partial<Pick<AgentMessage, "inputTokens" | "outputTokens" | "totalTokens" | "reasoning" | "reasoningTokens" | "audioTokens" | "videoTokens" | "cost" | "tps" | "toolCalls" | "provider" | "model" | "apiTransport" | "responseId">>
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

  // Provider auth
  providerAuthStates: ProviderAuthState[];
  subAgents: SubAgentDefinition[];
  refreshProviderAuthStates: () => Promise<void>;
  validateProvider: (providerId: string, baseUrl: string, apiKey: string, authSource: string) => Promise<{ valid: boolean; error?: string; models?: unknown[] }>;
  loginProvider: (providerId: string, apiKey: string, baseUrl?: string) => Promise<void>;
  logoutProvider: (providerId: string) => Promise<void>;
  addSubAgent: (def: Omit<SubAgentDefinition, "id" | "created_at">) => Promise<void>;
  removeSubAgent: (id: string) => Promise<void>;
  updateSubAgent: (def: SubAgentDefinition) => Promise<void>;
  refreshSubAgents: () => Promise<void>;

  // Concierge
  conciergeConfig: {
    enabled: boolean;
    detail_level: string;
    provider?: string;
    model?: string;
    auto_cleanup_on_navigate: boolean;
  };
  conciergeWelcome: {
    content: string;
    actions: Array<{ label: string; action_type: string; thread_id?: string }>;
  } | null;
  refreshConciergeConfig: () => Promise<void>;
  updateConciergeConfig: (config: Record<string, unknown>) => Promise<void>;
  dismissConciergeWelcome: () => Promise<void>;

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

function getBridge(): AmuxBridge | null {
  return (window as any).tamux ?? (window as any).amux ?? null;
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

type AgentDbApi = {
  dbCreateThread?: (thread: AgentDbThreadRecord) => Promise<boolean>;
  dbDeleteThread?: (id: string) => Promise<boolean>;
  dbListThreads?: () => Promise<AgentDbThreadRecord[]>;
  dbGetThread?: (id: string) => Promise<{ thread: AgentDbThreadRecord | null; messages: AgentDbMessageRecord[] }>;
  dbAddMessage?: (message: AgentDbMessageRecord) => Promise<boolean>;
  dbListMessages?: (threadId: string, limit?: number | null) => Promise<AgentDbMessageRecord[]>;
};

type RemoteAgentMessageRecord = {
  role?: AgentRole;
  content?: string;
  provider?: string | null;
  model?: string | null;
  api_transport?: string | null;
  response_id?: string | null;
  tool_calls?: ToolCall[] | null;
  tool_name?: string | null;
  tool_call_id?: string | null;
  tool_arguments?: string | null;
  tool_status?: string | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  reasoning?: string | null;
  timestamp?: number | null;
};

type RemoteAgentThreadRecord = {
  id?: string;
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

function getAgentDbApi(): AgentDbApi | null {
  const api = (window as any).tamux ?? (window as any).amux;
  if (!api) return null;
  return api as AgentDbApi;
}

function shouldPersistHistory(backend: AgentSettings["agentBackend"]): boolean {
  return backend === "legacy";
}

function buildHydratedRemoteMessage(
  threadId: string,
  message: RemoteAgentMessageRecord,
): AgentMessage {
  const provider = typeof message.provider === "string" ? message.provider : undefined;
  return {
    id: `msg_${++_msgId}`,
    threadId,
    createdAt: Number(message.timestamp ?? Date.now()),
    role: message.role ?? "assistant",
    content: typeof message.content === "string" ? message.content : "",
    provider,
    model: typeof message.model === "string" ? message.model : undefined,
    apiTransport: typeof message.api_transport === "string"
      ? normalizeApiTransport(
        typeof provider === "string" ? normalizeAgentProviderId(provider) : "openai",
        message.api_transport,
      )
      : undefined,
    responseId: typeof message.response_id === "string" ? message.response_id : undefined,
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
    inputTokens: Number(message.input_tokens ?? 0),
    outputTokens: Number(message.output_tokens ?? 0),
    totalTokens: Number(message.input_tokens ?? 0) + Number(message.output_tokens ?? 0),
    reasoning: typeof message.reasoning === "string" ? message.reasoning : undefined,
    isCompactionSummary: false,
    isStreaming: false,
  };
}

function buildHydratedRemoteThread(thread: RemoteAgentThreadRecord, agentName: string): {
  thread: AgentThread;
  messages: AgentMessage[];
} | null {
  if (typeof thread.id !== "string" || !thread.id.trim()) {
    return null;
  }

  const localThreadId = `thread_${++_threadId}`;
  const messages = Array.isArray(thread.messages)
    ? thread.messages.map((message) => buildHydratedRemoteMessage(localThreadId, message))
    : [];
  const totalInputTokens = Number(thread.total_input_tokens ?? 0);
  const totalOutputTokens = Number(thread.total_output_tokens ?? 0);

  return {
    thread: {
      id: localThreadId,
      daemonThreadId: thread.id,
      workspaceId: null,
      surfaceId: null,
      paneId: null,
      agentName,
      title: typeof thread.title === "string" && thread.title.trim()
        ? thread.title
        : "Conversation",
      createdAt: Number(thread.created_at ?? Date.now()),
      updatedAt: Number(thread.updated_at ?? Date.now()),
      messageCount: messages.length,
      totalInputTokens,
      totalOutputTokens,
      totalTokens: totalInputTokens + totalOutputTokens,
      compactionCount: 0,
      lastMessagePreview: messages[messages.length - 1]?.content?.slice(0, 100) ?? "",
      upstreamThreadId: typeof thread.upstream_thread_id === "string"
        ? thread.upstream_thread_id
        : null,
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
    metadata_json: JSON.stringify({
      upstreamThreadId: thread.upstreamThreadId ?? null,
      upstreamTransport: thread.upstreamTransport ?? null,
      upstreamProvider: thread.upstreamProvider ?? null,
      upstreamModel: thread.upstreamModel ?? null,
      upstreamAssistantId: thread.upstreamAssistantId ?? null,
    }),
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
      apiTransport: message.apiTransport ?? null,
      responseId: message.responseId ?? null,
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
      ? normalizeAgentProviderId(metadata.upstreamProvider)
      : null,
    upstreamModel: typeof metadata.upstreamModel === "string" ? metadata.upstreamModel : null,
    upstreamAssistantId: typeof metadata.upstreamAssistantId === "string"
      ? metadata.upstreamAssistantId
      : null,
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
    apiTransport: typeof metadata.apiTransport === "string"
      ? normalizeApiTransport(
        typeof message.provider === "string"
          ? normalizeAgentProviderId(message.provider)
          : "openai",
        metadata.apiTransport,
      )
      : undefined,
    responseId: typeof metadata.responseId === "string" ? metadata.responseId : undefined,
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
  providerAuthStates: [],
  subAgents: [],
  refreshProviderAuthStates: async () => {
    const bridge = getBridge();
    if (!bridge?.agentGetProviderAuthStates) return;
    try {
      const states = await bridge.agentGetProviderAuthStates();
      if (Array.isArray(states)) {
        set({ providerAuthStates: states as ProviderAuthState[] });
      }
    } catch { /* ignore */ }
  },
  validateProvider: async (providerId, baseUrl, apiKey, authSource) => {
    const bridge = getBridge();
    if (!bridge?.agentValidateProvider) return { valid: false, error: "Bridge not available" };
    try {
      return await bridge.agentValidateProvider(providerId, baseUrl, apiKey, authSource);
    } catch (e) {
      return { valid: false, error: String(e) };
    }
  },
  loginProvider: async (providerId, apiKey, baseUrl) => {
    const bridge = getBridge();
    if (!bridge?.agentLoginProvider) return;
    try {
      const result = await bridge.agentLoginProvider(providerId, apiKey, baseUrl);
      // The daemon returns updated auth states directly.
      if (Array.isArray(result)) {
        set({ providerAuthStates: result as ProviderAuthState[] });
      }
    } catch { /* ignore */ }
  },
  logoutProvider: async (providerId) => {
    const bridge = getBridge();
    if (!bridge?.agentLogoutProvider) return;
    try {
      const result = await bridge.agentLogoutProvider(providerId);
      if (Array.isArray(result)) {
        set({ providerAuthStates: result as ProviderAuthState[] });
      }
    } catch { /* ignore */ }
  },
  addSubAgent: async (def) => {
    const bridge = getBridge();
    if (!bridge?.agentSetSubAgent) return;
    const full: SubAgentDefinition = {
      ...def,
      id: `subagent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      created_at: Math.floor(Date.now() / 1000),
    };
    try {
      await bridge.agentSetSubAgent(JSON.stringify(full));
      await get().refreshSubAgents();
    } catch { /* ignore */ }
  },
  removeSubAgent: async (id) => {
    const bridge = getBridge();
    if (!bridge?.agentRemoveSubAgent) return;
    try {
      await bridge.agentRemoveSubAgent(id);
      await get().refreshSubAgents();
    } catch { /* ignore */ }
  },
  updateSubAgent: async (def) => {
    const bridge = getBridge();
    if (!bridge?.agentSetSubAgent) return;
    try {
      await bridge.agentSetSubAgent(JSON.stringify(def));
      await get().refreshSubAgents();
    } catch { /* ignore */ }
  },
  refreshSubAgents: async () => {
    const bridge = getBridge();
    if (!bridge?.agentListSubAgents) return;
    try {
      const list = await bridge.agentListSubAgents();
      if (Array.isArray(list)) {
        set({ subAgents: list as SubAgentDefinition[] });
      }
    } catch { /* ignore */ }
  },

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
      upstreamThreadId: null,
      upstreamTransport: undefined,
      upstreamProvider: null,
      upstreamModel: null,
      upstreamAssistantId: null,
    };
    set((s) => {
      const next: AgentChatState = {
        threads: [thread, ...s.threads],
        messages: { ...s.messages, [id]: [] },
        todos: { ...s.todos, [id]: [] },
        activeThreadId: id,
      };
      if (shouldPersistHistory(get().agentSettings.agentBackend)) {
        persistDaemonThreadMap(next.threads);
        scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
        void getAgentDbApi()?.dbCreateThread?.(serializeThread(thread));
      }
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
      if (shouldPersistHistory(get().agentSettings.agentBackend)) {
        persistDaemonThreadMap(next.threads);
        void getAgentDbApi()?.dbDeleteThread?.(id);
      }
      return next;
    });
  },

  setActiveThread: (id) => {
    set({ activeThreadId: id });
    if (shouldPersistHistory(get().agentSettings.agentBackend)) {
      scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
    }
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
      if (shouldPersistHistory(get().agentSettings.agentBackend)) {
        void (async () => {
          const api = getAgentDbApi();
          if (updatedThread) await api?.dbCreateThread?.(serializeThread(updatedThread));
          await api?.dbAddMessage?.(serializeMessage(full));
        })();
      }
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
        apiTransport: meta?.apiTransport ?? last.apiTransport,
        responseId: meta?.responseId ?? last.responseId,
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
      if (shouldPersistHistory(get().agentSettings.agentBackend)) {
        void (async () => {
          const api = getAgentDbApi();
          if (updatedThread) await api?.dbCreateThread?.(serializeThread(updatedThread));
          await api?.dbAddMessage?.(serializeMessage(updatedLast));
        })();
      }
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
      if (shouldPersistHistory(get().agentSettings.agentBackend)) {
        persistDaemonThreadMap(threads);
      }
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

  conciergeConfig: {
    enabled: true,
    detail_level: "proactive_triage",
    auto_cleanup_on_navigate: true,
  },
  conciergeWelcome: null,
  refreshConciergeConfig: async () => {
    const bridge = getBridge();
    if (!bridge?.agentGetConciergeConfig) return;
    try {
      const config = await bridge.agentGetConciergeConfig();
      if (config && typeof config === "object") {
        set({ conciergeConfig: config as any });
      }
    } catch { /* ignore */ }
  },
  updateConciergeConfig: async (config) => {
    const bridge = getBridge();
    if (!bridge?.agentSetConciergeConfig) return;
    try {
      await bridge.agentSetConciergeConfig(config);
      if (bridge.agentGetConciergeConfig) {
        const refreshed = await bridge.agentGetConciergeConfig();
        if (refreshed && typeof refreshed === "object") {
          set({ conciergeConfig: refreshed as any });
          return;
        }
      }
      set({ conciergeConfig: config as any });
    } catch { /* ignore */ }
  },
  dismissConciergeWelcome: async () => {
    const bridge = getBridge();
    if (!bridge?.agentDismissConciergeWelcome) return;
    try {
      await bridge.agentDismissConciergeWelcome();
      set({ conciergeWelcome: null });
    } catch { /* ignore */ }
  },
  getThreadsForPane: (paneId) => get().threads.filter((t) => t.paneId === paneId),
}));

export async function hydrateAgentStore(): Promise<void> {
  const diskState = await readPersistedJson<AgentSettings>(AGENT_SETTINGS_FILE);
  const configuredBackend = normalizeAgentBackend(diskState?.agentBackend);
  if (diskState) {
    const merged: AgentSettings = {
      ...DEFAULT_AGENT_SETTINGS,
      ...diskState,
      activeProvider: normalizeAgentProviderId(diskState.activeProvider),
      agentBackend: normalizeAgentBackend(diskState.agentBackend),
      featherless: normalizeProviderConfig("featherless", DEFAULT_AGENT_SETTINGS.featherless, diskState.featherless),
      openai: normalizeProviderConfig("openai", DEFAULT_AGENT_SETTINGS.openai, diskState.openai),
      qwen: normalizeProviderConfig("qwen", DEFAULT_AGENT_SETTINGS.qwen, diskState.qwen),
      "qwen-deepinfra": normalizeProviderConfig("qwen-deepinfra", DEFAULT_AGENT_SETTINGS["qwen-deepinfra"], diskState["qwen-deepinfra"]),
      kimi: normalizeProviderConfig("kimi", DEFAULT_AGENT_SETTINGS.kimi, diskState.kimi),
      "kimi-coding-plan": normalizeProviderConfig("kimi-coding-plan", DEFAULT_AGENT_SETTINGS["kimi-coding-plan"], diskState["kimi-coding-plan"]),
      "z.ai": normalizeProviderConfig("z.ai", DEFAULT_AGENT_SETTINGS["z.ai"], diskState["z.ai"]),
      "z.ai-coding-plan": normalizeProviderConfig("z.ai-coding-plan", DEFAULT_AGENT_SETTINGS["z.ai-coding-plan"], diskState["z.ai-coding-plan"]),
      openrouter: normalizeProviderConfig("openrouter", DEFAULT_AGENT_SETTINGS.openrouter, diskState.openrouter),
      cerebras: normalizeProviderConfig("cerebras", DEFAULT_AGENT_SETTINGS.cerebras, diskState.cerebras),
      together: normalizeProviderConfig("together", DEFAULT_AGENT_SETTINGS.together, diskState.together),
      groq: normalizeProviderConfig("groq", DEFAULT_AGENT_SETTINGS.groq, diskState.groq),
      ollama: normalizeProviderConfig("ollama", DEFAULT_AGENT_SETTINGS.ollama, diskState.ollama),
      chutes: normalizeProviderConfig("chutes", DEFAULT_AGENT_SETTINGS.chutes, diskState.chutes),
      huggingface: normalizeProviderConfig("huggingface", DEFAULT_AGENT_SETTINGS.huggingface, diskState.huggingface),
      minimax: normalizeProviderConfig("minimax", DEFAULT_AGENT_SETTINGS.minimax, diskState.minimax),
      "minimax-coding-plan": normalizeProviderConfig("minimax-coding-plan", DEFAULT_AGENT_SETTINGS["minimax-coding-plan"], diskState["minimax-coding-plan"]),
      "alibaba-coding-plan": normalizeProviderConfig("alibaba-coding-plan", DEFAULT_AGENT_SETTINGS["alibaba-coding-plan"], diskState["alibaba-coding-plan"]),
      "opencode-zen": normalizeProviderConfig("opencode-zen", DEFAULT_AGENT_SETTINGS["opencode-zen"], diskState["opencode-zen"]),
      custom: normalizeProviderConfig("custom", DEFAULT_AGENT_SETTINGS.custom, diskState.custom),
    };
    useAgentStore.setState({ agentSettings: merged });
  }

  if (!shouldPersistHistory(configuredBackend)) {
    const amux = (window as any).tamux ?? (window as any).amux;
    if (amux?.agentListThreads) {
      const remoteThreads = await amux.agentListThreads().catch(() => []);
      if (Array.isArray(remoteThreads) && remoteThreads.length > 0) {
        const messages: Record<string, AgentMessage[]> = {};
        const threads: AgentThread[] = [];
        for (const remoteThread of remoteThreads) {
          const hydrated = buildHydratedRemoteThread(
            (remoteThread ?? {}) as RemoteAgentThreadRecord,
            useAgentStore.getState().agentSettings.agentName,
          );
          if (!hydrated) continue;
          threads.push(hydrated.thread);
          messages[hydrated.thread.id] = hydrated.messages;
        }

        if (threads.length > 0) {
          const sortedThreads = threads.sort((a, b) => b.updatedAt - a.updatedAt);
          const hydrated: AgentChatState = {
            threads: sortedThreads,
            messages,
            todos: {},
            activeThreadId: sortedThreads[0]?.id ?? null,
          };
          syncChatCounters(hydrated);
          useAgentStore.setState(hydrated);
        }
      }
    }
    return;
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
