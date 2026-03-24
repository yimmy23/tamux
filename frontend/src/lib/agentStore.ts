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
  agent_name: string;
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

function normalizeAgentBackend(value: unknown): AgentSettings["agent_backend"] {
  if (typeof value === "string" && (VALID_AGENT_BACKENDS as readonly string[]).includes(value)) {
    return value as AgentSettings["agent_backend"];
  }
  return "daemon";
}

export interface AgentProviderConfig {
  base_url: string;
  model: string;
  custom_model_name: string;
  api_key: string;
  assistant_id: string;
  api_transport: ApiTransportMode;
  auth_source: AuthSource;
  context_window_tokens: number | null;
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
  const auth_source = normalizeAuthSource(providerId, value?.auth_source ?? fallback.auth_source);
  const requestedModel = typeof value?.model === "string" ? value.model.trim() : fallback.model.trim();
  const supportedModels = getProviderModels(providerId, auth_source);
  const matchesKnownModel = requestedModel && supportedModels.some((entry) => entry.id === requestedModel);
  const model = requestedModel
    ? requestedModel
    : getDefaultModelForProvider(providerId, auth_source);
  const custom_model_name = typeof value?.custom_model_name === "string"
    ? value.custom_model_name.trim()
    : "";
  return {
    ...fallback,
    ...(value ?? {}),
    model,
    custom_model_name: matchesKnownModel ? "" : (custom_model_name || (requestedModel && !matchesKnownModel ? requestedModel : "")),
    assistant_id: typeof value?.assistant_id === "string" ? value.assistant_id : fallback.assistant_id,
    api_transport: normalizeApiTransport(providerId, value?.api_transport ?? fallback.api_transport),
    auth_source,
    context_window_tokens:
      typeof value?.context_window_tokens === "number" && Number.isFinite(value.context_window_tokens)
        ? Math.max(1000, Math.trunc(value.context_window_tokens))
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
  { id: "qwen3-coder-plus", name: "Qwen3 Coder Plus", contextWindow: 997952 },
  { id: "qwen3-coder-next", name: "Qwen3 Coder Next", contextWindow: 204800 },
  { id: "qwen3.5-plus", name: "Qwen3.5 Plus", contextWindow: 983616 },
  { id: "glm-5", name: "GLM-5", contextWindow: 202752 },
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
  { id: "alibaba-coding-plan", name: "Alibaba Coding Plan", defaultBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/v1", defaultModel: "qwen3.5-plus", apiType: "openai", authMethod: "bearer", models: ALIBABA_CODING_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
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
  auth_source?: AuthSource,
): ModelDefinition[] {
  if (providerId === "openai" && auth_source === "chatgpt_subscription") {
    return OPENAI_CHATGPT_SUBSCRIPTION_MODELS;
  }
  return getProviderDefinition(providerId)?.models ?? [];
}

export function getDefaultModelForProvider(
  providerId: AgentProviderId,
  auth_source?: AuthSource,
): string {
  const models = getProviderModels(providerId, auth_source);
  if (models.length > 0) {
    return models[0].id;
  }
  return getProviderDefinition(providerId)?.defaultModel ?? "";
}

export function getModelDefinition(
  providerId: AgentProviderId,
  modelId: string,
  auth_source?: AuthSource,
): ModelDefinition | undefined {
  const trimmed = modelId.trim();
  if (!trimmed) return undefined;
  return getProviderModels(providerId, auth_source).find((model) => model.id === trimmed);
}

export function getEffectiveContextWindow(
  providerId: AgentProviderId,
  config: Pick<AgentProviderConfig, "model" | "context_window_tokens" | "auth_source">,
): number {
  if (providerId === "custom") {
    if (typeof config.context_window_tokens === "number" && config.context_window_tokens > 0) {
      return Math.max(1000, Math.trunc(config.context_window_tokens));
    }
    return 128_000;
  }

  return getModelDefinition(providerId, config.model, config.auth_source)?.contextWindow ?? 128_000;
}

export function providerSupportsResponseContinuity(providerId: AgentProviderId): boolean {
  return Boolean(getProviderDefinition(providerId)?.supportsResponseContinuity);
}

function isAlibabaCodingPlanAnthropicBaseUrl(baseUrl: string): boolean {
  const lower = (baseUrl || "").trim().toLowerCase();
  return lower.includes("dashscope.aliyuncs.com") && lower.includes("/apps/anthropic");
}

export function getProviderApiType(providerId: AgentProviderId, model: string, configuredUrl: string = ""): ApiType {
  const def = getProviderDefinition(providerId);
  if (!def) return "openai";

  if (providerId === "alibaba-coding-plan" && isAlibabaCodingPlanAnthropicBaseUrl(configuredUrl)) {
    return "anthropic";
  }

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
  api_transport?: ApiTransportMode;
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
  agent_name: string;
  handler: string;
  additionalHandlers: string[];
  system_prompt: string;

  active_provider: AgentProviderId;
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

  enable_bash_tool: boolean;
  enable_vision_tool: boolean;
  enable_web_browsing_tool: boolean;
  bash_timeout_seconds: number;
  enable_web_search_tool: boolean;
  search_provider: "none" | "firecrawl" | "exa" | "tavily";
  firecrawl_api_key: string;
  exa_api_key: string;
  tavily_api_key: string;
  search_max_results: number;
  search_timeout_secs: number;

  enable_streaming: boolean;
  enable_conversation_memory: boolean;
  enable_honcho_memory: boolean;
  honcho_api_key: string;
  honcho_base_url: string;
  honcho_workspace_id: string;
  anticipatory_enabled: boolean;
  anticipatory_morning_brief: boolean;
  anticipatory_predictive_hydration: boolean;
  anticipatory_stuck_detection: boolean;
  operator_model_enabled: boolean;
  operator_model_allow_message_statistics: boolean;
  operator_model_allow_approval_learning: boolean;
  operator_model_allow_attention_tracking: boolean;
  operator_model_allow_implicit_feedback: boolean;
  collaboration_enabled: boolean;
  compliance_mode: "standard" | "soc2" | "hipaa" | "fedramp";
  compliance_retention_days: number;
  compliance_sign_all_events: boolean;
  tool_synthesis_enabled: boolean;
  tool_synthesis_require_activation: boolean;
  tool_synthesis_max_generated_tools: number;
  gateway_enabled: boolean;
  slack_token: string;
  slack_channel_filter: string;
  telegram_token: string;
  telegram_allowed_chats: string;
  discord_token: string;
  discord_channel_filter: string;
  discord_allowed_users: string;
  whatsapp_token: string;
  whatsapp_phone_id: string;
  whatsapp_allowed_contacts: string;
  gateway_command_prefix: string;

  chatFontFamily: string;
  chatFontSize: number;

  reasoning_effort: "none" | "minimal" | "low" | "medium" | "high" | "xhigh";

  auto_compact_context: boolean;
  max_context_messages: number;
  max_tool_loops: number;
  max_retries: number;
  retry_delay_ms: number;
  context_window_tokens: number;
  context_budget_tokens: number;
  compact_threshold_pct: number;
  keep_recent_on_compact: number;

  // Agent backend: "daemon" runs LLM in tamux-daemon, "openclaw"/"hermes" route
  // through external agent processes, "legacy" uses frontend
  agent_backend: "daemon" | "openclaw" | "hermes" | "legacy";
}

export const DEFAULT_AGENT_SETTINGS: AgentSettings = {
  enabled: false,
  agent_name: "assistant",
  handler: "/agent",
  additionalHandlers: [],
  system_prompt: "You are tamux, an agentic terminal multiplexer assistant. You can execute terminal commands, check system resources, and send messages to connected chat platforms (Slack, Discord, Telegram, WhatsApp) via the gateway. Use your tools proactively when the user asks you to perform actions. Be concise and direct.",

  active_provider: "openai",
  featherless: { base_url: "https://api.featherless.ai/v1", model: "meta-llama/Llama-3.3-70B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  openai: { base_url: "https://api.openai.com/v1", model: "gpt-5.4", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  qwen: { base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-max", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "native_assistant", auth_source: "api_key", context_window_tokens: null },
  "qwen-deepinfra": { base_url: "https://api.deepinfra.com/v1/openai", model: "Qwen/Qwen2.5-72B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  kimi: { base_url: "https://api.moonshot.ai/v1", model: "moonshot-v1-32k", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "kimi-coding-plan": { base_url: "https://api.kimi.com/coding/v1", model: "kimi-for-coding", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "z.ai": { base_url: "https://api.z.ai/api/paas/v4", model: "glm-4-plus", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "z.ai-coding-plan": { base_url: "https://api.z.ai/api/coding/paas/v4", model: "glm-5", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  openrouter: { base_url: "https://openrouter.ai/api/v1", model: "anthropic/claude-sonnet-4", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  cerebras: { base_url: "https://api.cerebras.ai/v1", model: "llama-3.3-70b", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  together: { base_url: "https://api.together.xyz/v1", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  groq: { base_url: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  ollama: { base_url: "http://localhost:11434/v1", model: "llama3.1", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  chutes: { base_url: "https://llm.chutes.ai/v1", model: "deepseek-ai/DeepSeek-V3", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  huggingface: { base_url: "https://api-inference.huggingface.co/v1", model: "meta-llama/Llama-3.3-70B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  minimax: { base_url: "https://api.minimax.io/anthropic", model: "MiniMax-M1-80k", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "minimax-coding-plan": { base_url: "https://api.minimax.io/anthropic", model: "MiniMax-M2.7", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "alibaba-coding-plan": { base_url: "https://coding-intl.dashscope.aliyuncs.com/v1", model: "qwen3.5-plus", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "opencode-zen": { base_url: "https://opencode.ai/zen/v1", model: "claude-sonnet-4-5", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  custom: { base_url: "", model: "", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: 128_000 },

  enable_bash_tool: true,
  enable_vision_tool: false,
  enable_web_browsing_tool: false,
  bash_timeout_seconds: 30,
  enable_web_search_tool: false,
  search_provider: "none",
  firecrawl_api_key: "",
  exa_api_key: "",
  tavily_api_key: "",
  search_max_results: 8,
  search_timeout_secs: 20,

  enable_streaming: true,
  enable_conversation_memory: true,
  enable_honcho_memory: false,
  honcho_api_key: "",
  honcho_base_url: "",
  honcho_workspace_id: "tamux",
  anticipatory_enabled: false,
  anticipatory_morning_brief: false,
  anticipatory_predictive_hydration: false,
  anticipatory_stuck_detection: false,
  operator_model_enabled: false,
  operator_model_allow_message_statistics: false,
  operator_model_allow_approval_learning: false,
  operator_model_allow_attention_tracking: false,
  operator_model_allow_implicit_feedback: false,
  collaboration_enabled: false,
  compliance_mode: "standard",
  compliance_retention_days: 30,
  compliance_sign_all_events: false,
  tool_synthesis_enabled: false,
  tool_synthesis_require_activation: true,
  tool_synthesis_max_generated_tools: 24,
  gateway_enabled: false,
  slack_token: "",
  slack_channel_filter: "",
  telegram_token: "",
  telegram_allowed_chats: "",
  discord_token: "",
  discord_channel_filter: "",
  discord_allowed_users: "",
  whatsapp_token: "",
  whatsapp_phone_id: "",
  whatsapp_allowed_contacts: "",
  gateway_command_prefix: "!tamux",

  chatFontFamily: "Cascadia Code",
  chatFontSize: 13,

  reasoning_effort: "high",

  auto_compact_context: true,
  max_context_messages: 100,
  max_tool_loops: 0,
  max_retries: 3,
  retry_delay_ms: 2000,
  context_window_tokens: 128000,
  context_budget_tokens: 100000,
  compact_threshold_pct: 80,
  keep_recent_on_compact: 10,

  agent_backend: "daemon",
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
  agentSettingsHydrated: boolean;
  agentSettingsDirty: boolean;
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
    meta?: Partial<Pick<AgentMessage, "inputTokens" | "outputTokens" | "totalTokens" | "reasoning" | "reasoningTokens" | "audioTokens" | "videoTokens" | "cost" | "tps" | "toolCalls" | "provider" | "model" | "api_transport" | "responseId">>
  ) => void;
  getThreadMessages: (threadId: string) => AgentMessage[];
  deleteMessage: (threadId: string, messageId: string) => void;
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  getThreadTodos: (threadId: string) => AgentTodoItem[];
  setThreadDaemonId: (threadId: string, daemonThreadId: string | null) => void;

  // Panel
  toggleAgentPanel: () => void;
  setSearchQuery: (q: string) => void;

  // Settings
  updateAgentSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
  resetAgentSettings: () => void;
  refreshAgentSettingsFromDaemon: () => Promise<boolean>;
  markAgentSettingsSynced: () => void;

  // Provider auth
  providerAuthStates: ProviderAuthState[];
  subAgents: SubAgentDefinition[];
  refreshProviderAuthStates: () => Promise<void>;
  validateProvider: (providerId: string, base_url: string, api_key: string, auth_source: string) => Promise<{ valid: boolean; error?: string; models?: unknown[] }>;
  loginProvider: (providerId: string, api_key: string, base_url?: string) => Promise<void>;
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

  // Gateway status
  gatewayStatuses: Record<string, { status: string; lastError?: string; consecutiveFailures?: number; updatedAt: number }>;
  setGatewayStatus: (platform: string, status: string, lastError?: string, consecutiveFailures?: number) => void;

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

const AGENT_CHAT_FILE = "agent-chat.json";
const AGENT_DAEMON_THREAD_MAP_FILE = "agent-daemon-thread-map.json";
const AGENT_ACTIVE_THREAD_FILE = "agent-active-thread.json";

type AgentChatState = {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>;
  todos: Record<string, AgentTodoItem[]>;
  activeThreadId: string | null;
};

type DiskAgentSettings = Partial<AgentSettings> & {
  provider?: string;
  base_url?: string;
  model?: string;
  api_key?: string;
  assistant_id?: string;
  auth_source?: string;
  api_transport?: string;
  reasoning_effort?: AgentSettings["reasoning_effort"] | string;
  system_prompt?: string;
  auto_compact_context?: boolean;
  max_context_messages?: number;
  max_tool_loops?: number;
  max_retries?: number;
  retry_delay_ms?: number;
  context_window_tokens?: number;
  context_budget_tokens?: number;
  compact_threshold_pct?: number;
  keep_recent_on_compact?: number;
  agent_backend?: AgentSettings["agent_backend"] | string;
  enable_honcho_memory?: boolean;
  honcho_api_key?: string;
  honcho_base_url?: string;
  honcho_workspace_id?: string;
  providers?: Record<string, Partial<AgentProviderConfig> & {
    base_url?: string;
    custom_model_name?: string;
    api_key?: string;
    assistant_id?: string;
    auth_source?: string;
    api_transport?: string;
    context_window_tokens?: number;
  }>;
  tools?: {
    bash?: boolean;
    vision?: boolean;
    web_browse?: boolean;
    web_search?: boolean;
  };
  anticipatory?: {
    enabled?: boolean;
    morning_brief?: boolean;
    predictive_hydration?: boolean;
    stuck_detection?: boolean;
  };
  operator_model?: {
    enabled?: boolean;
    allow_message_statistics?: boolean;
    allow_approval_learning?: boolean;
    allow_attention_tracking?: boolean;
    allow_implicit_feedback?: boolean;
  };
  collaboration?: {
    enabled?: boolean;
  };
  compliance?: {
    mode?: AgentSettings["compliance_mode"];
    retention_days?: number;
    sign_all_events?: boolean;
  };
  tool_synthesis?: {
    enabled?: boolean;
    require_activation?: boolean;
    max_generated_tools?: number;
  };
  gateway?: {
    enabled?: boolean;
    slack_token?: string;
    slack_channel_filter?: string;
    telegram_token?: string;
    telegram_allowed_chats?: string;
    discord_token?: string;
    discord_channel_filter?: string;
    discord_allowed_users?: string;
    whatsapp_token?: string;
    whatsapp_phone_id?: string;
    whatsapp_allowed_contacts?: string;
    command_prefix?: string;
  };
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
  dbDeleteMessage?: (threadId: string, messageId: string) => Promise<boolean>;
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

function shouldPersistHistory(backend: AgentSettings["agent_backend"]): boolean {
  const bridge = getBridge();
  return backend === "legacy" && !bridge?.agentSendMessage;
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
    api_transport: typeof message.api_transport === "string"
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

function buildHydratedRemoteThread(thread: RemoteAgentThreadRecord, agent_name: string): {
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
      agent_name,
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

function providerConfigFromRaw(
  providerId: AgentProviderId,
  source: DiskAgentSettings | null | undefined,
): AgentProviderConfig {
  const providerMapValue = source?.providers?.[providerId];
  const flatValue = source?.[providerId] as Partial<AgentProviderConfig> | undefined;
  const mergedValue: Partial<AgentProviderConfig> = {
    ...(flatValue ?? {}),
    base_url:
      providerMapValue?.base_url
      ?? providerMapValue?.base_url
      ?? flatValue?.base_url,
    custom_model_name:
      providerMapValue?.custom_model_name
      ?? providerMapValue?.custom_model_name
      ?? flatValue?.custom_model_name,
    api_key:
      providerMapValue?.api_key
      ?? providerMapValue?.api_key
      ?? flatValue?.api_key,
    assistant_id:
      providerMapValue?.assistant_id
      ?? providerMapValue?.assistant_id
      ?? flatValue?.assistant_id,
    auth_source: (
      providerMapValue?.auth_source
      ?? providerMapValue?.auth_source
      ?? flatValue?.auth_source
    ) as AuthSource | undefined,
    api_transport: (
      providerMapValue?.api_transport
      ?? providerMapValue?.api_transport
      ?? flatValue?.api_transport
    ) as ApiTransportMode | undefined,
    context_window_tokens:
      typeof providerMapValue?.context_window_tokens === "number"
        ? providerMapValue.context_window_tokens
        : flatValue?.context_window_tokens,
  };
  return normalizeProviderConfig(providerId, DEFAULT_AGENT_SETTINGS[providerId], mergedValue);
}

function normalizeAgentSettingsFromSource(source: DiskAgentSettings): AgentSettings {
  const active_provider = normalizeAgentProviderId(
    source.active_provider ?? source.provider,
  );
  const active_providerConfig = providerConfigFromRaw(active_provider, source);
  return {
    ...DEFAULT_AGENT_SETTINGS,
    ...source,
    active_provider,
    agent_backend: normalizeAgentBackend(source.agent_backend ?? source.agent_backend),
    featherless: providerConfigFromRaw("featherless", source),
    openai: providerConfigFromRaw("openai", source),
    qwen: providerConfigFromRaw("qwen", source),
    "qwen-deepinfra": providerConfigFromRaw("qwen-deepinfra", source),
    kimi: providerConfigFromRaw("kimi", source),
    "kimi-coding-plan": providerConfigFromRaw("kimi-coding-plan", source),
    "z.ai": providerConfigFromRaw("z.ai", source),
    "z.ai-coding-plan": providerConfigFromRaw("z.ai-coding-plan", source),
    openrouter: providerConfigFromRaw("openrouter", source),
    cerebras: providerConfigFromRaw("cerebras", source),
    together: providerConfigFromRaw("together", source),
    groq: providerConfigFromRaw("groq", source),
    ollama: providerConfigFromRaw("ollama", source),
    chutes: providerConfigFromRaw("chutes", source),
    huggingface: providerConfigFromRaw("huggingface", source),
    minimax: providerConfigFromRaw("minimax", source),
    "minimax-coding-plan": providerConfigFromRaw("minimax-coding-plan", source),
    "alibaba-coding-plan": providerConfigFromRaw("alibaba-coding-plan", source),
    "opencode-zen": providerConfigFromRaw("opencode-zen", source),
    custom: providerConfigFromRaw("custom", source),
    system_prompt: source.system_prompt ?? source.system_prompt ?? DEFAULT_AGENT_SETTINGS.system_prompt,
    reasoning_effort: (source.reasoning_effort ?? source.reasoning_effort ?? DEFAULT_AGENT_SETTINGS.reasoning_effort) as AgentSettings["reasoning_effort"],
    auto_compact_context: source.auto_compact_context ?? source.auto_compact_context ?? DEFAULT_AGENT_SETTINGS.auto_compact_context,
    max_context_messages: source.max_context_messages ?? source.max_context_messages ?? DEFAULT_AGENT_SETTINGS.max_context_messages,
    max_tool_loops: source.max_tool_loops ?? source.max_tool_loops ?? DEFAULT_AGENT_SETTINGS.max_tool_loops,
    max_retries: source.max_retries ?? source.max_retries ?? DEFAULT_AGENT_SETTINGS.max_retries,
    retry_delay_ms: source.retry_delay_ms ?? source.retry_delay_ms ?? DEFAULT_AGENT_SETTINGS.retry_delay_ms,
    context_window_tokens: source.context_window_tokens ?? source.context_window_tokens ?? DEFAULT_AGENT_SETTINGS.context_window_tokens,
    context_budget_tokens: source.context_budget_tokens ?? source.context_budget_tokens ?? DEFAULT_AGENT_SETTINGS.context_budget_tokens,
    compact_threshold_pct: source.compact_threshold_pct ?? source.compact_threshold_pct ?? DEFAULT_AGENT_SETTINGS.compact_threshold_pct,
    keep_recent_on_compact: source.keep_recent_on_compact ?? source.keep_recent_on_compact ?? DEFAULT_AGENT_SETTINGS.keep_recent_on_compact,
    enable_honcho_memory: source.enable_honcho_memory ?? source.enable_honcho_memory ?? DEFAULT_AGENT_SETTINGS.enable_honcho_memory,
    honcho_api_key: source.honcho_api_key ?? source.honcho_api_key ?? DEFAULT_AGENT_SETTINGS.honcho_api_key,
    honcho_base_url: source.honcho_base_url ?? source.honcho_base_url ?? DEFAULT_AGENT_SETTINGS.honcho_base_url,
    honcho_workspace_id: source.honcho_workspace_id ?? source.honcho_workspace_id ?? DEFAULT_AGENT_SETTINGS.honcho_workspace_id,
    enable_bash_tool: source.enable_bash_tool ?? source.tools?.bash ?? DEFAULT_AGENT_SETTINGS.enable_bash_tool,
    enable_vision_tool: source.enable_vision_tool ?? source.tools?.vision ?? DEFAULT_AGENT_SETTINGS.enable_vision_tool,
    enable_web_browsing_tool: source.enable_web_browsing_tool ?? source.tools?.web_browse ?? DEFAULT_AGENT_SETTINGS.enable_web_browsing_tool,
    enable_web_search_tool: source.enable_web_search_tool ?? source.tools?.web_search ?? DEFAULT_AGENT_SETTINGS.enable_web_search_tool,
    anticipatory_enabled: source.anticipatory?.enabled ?? source.anticipatory_enabled ?? DEFAULT_AGENT_SETTINGS.anticipatory_enabled,
    anticipatory_morning_brief: source.anticipatory?.morning_brief ?? source.anticipatory_morning_brief ?? DEFAULT_AGENT_SETTINGS.anticipatory_morning_brief,
    anticipatory_predictive_hydration: source.anticipatory?.predictive_hydration ?? source.anticipatory_predictive_hydration ?? DEFAULT_AGENT_SETTINGS.anticipatory_predictive_hydration,
    anticipatory_stuck_detection: source.anticipatory?.stuck_detection ?? source.anticipatory_stuck_detection ?? DEFAULT_AGENT_SETTINGS.anticipatory_stuck_detection,
    operator_model_enabled: source.operator_model?.enabled ?? source.operator_model_enabled ?? DEFAULT_AGENT_SETTINGS.operator_model_enabled,
    operator_model_allow_message_statistics: source.operator_model?.allow_message_statistics ?? source.operator_model_allow_message_statistics ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_message_statistics,
    operator_model_allow_approval_learning: source.operator_model?.allow_approval_learning ?? source.operator_model_allow_approval_learning ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_approval_learning,
    operator_model_allow_attention_tracking: source.operator_model?.allow_attention_tracking ?? source.operator_model_allow_attention_tracking ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_attention_tracking,
    operator_model_allow_implicit_feedback: source.operator_model?.allow_implicit_feedback ?? source.operator_model_allow_implicit_feedback ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_implicit_feedback,
    collaboration_enabled: source.collaboration?.enabled ?? source.collaboration_enabled ?? DEFAULT_AGENT_SETTINGS.collaboration_enabled,
    compliance_mode: source.compliance?.mode ?? source.compliance_mode ?? DEFAULT_AGENT_SETTINGS.compliance_mode,
    compliance_retention_days: source.compliance?.retention_days ?? source.compliance_retention_days ?? DEFAULT_AGENT_SETTINGS.compliance_retention_days,
    compliance_sign_all_events: source.compliance?.sign_all_events ?? source.compliance_sign_all_events ?? DEFAULT_AGENT_SETTINGS.compliance_sign_all_events,
    tool_synthesis_enabled: source.tool_synthesis?.enabled ?? source.tool_synthesis_enabled ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_enabled,
    tool_synthesis_require_activation: source.tool_synthesis?.require_activation ?? source.tool_synthesis_require_activation ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_require_activation,
    tool_synthesis_max_generated_tools: source.tool_synthesis?.max_generated_tools ?? source.tool_synthesis_max_generated_tools ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_max_generated_tools,
    gateway_enabled: source.gateway?.enabled ?? DEFAULT_AGENT_SETTINGS.gateway_enabled,
    slack_token: source.gateway?.slack_token ?? DEFAULT_AGENT_SETTINGS.slack_token,
    slack_channel_filter: source.gateway?.slack_channel_filter ?? DEFAULT_AGENT_SETTINGS.slack_channel_filter,
    telegram_token: source.gateway?.telegram_token ?? DEFAULT_AGENT_SETTINGS.telegram_token,
    telegram_allowed_chats: source.gateway?.telegram_allowed_chats ?? DEFAULT_AGENT_SETTINGS.telegram_allowed_chats,
    discord_token: source.gateway?.discord_token ?? DEFAULT_AGENT_SETTINGS.discord_token,
    discord_channel_filter: source.gateway?.discord_channel_filter ?? DEFAULT_AGENT_SETTINGS.discord_channel_filter,
    discord_allowed_users: source.gateway?.discord_allowed_users ?? DEFAULT_AGENT_SETTINGS.discord_allowed_users,
    whatsapp_token: source.gateway?.whatsapp_token ?? DEFAULT_AGENT_SETTINGS.whatsapp_token,
    whatsapp_phone_id: source.gateway?.whatsapp_phone_id ?? DEFAULT_AGENT_SETTINGS.whatsapp_phone_id,
    whatsapp_allowed_contacts: source.gateway?.whatsapp_allowed_contacts ?? DEFAULT_AGENT_SETTINGS.whatsapp_allowed_contacts,
    gateway_command_prefix: source.gateway?.command_prefix ?? DEFAULT_AGENT_SETTINGS.gateway_command_prefix,
    [active_provider]: {
      ...active_providerConfig,
      base_url: source.base_url ?? active_providerConfig.base_url,
      model: source.model ?? active_providerConfig.model,
      api_key: source.api_key ?? active_providerConfig.api_key,
      assistant_id: source.assistant_id ?? active_providerConfig.assistant_id,
      auth_source: normalizeAuthSource(active_provider, source.auth_source ?? active_providerConfig.auth_source),
      api_transport: normalizeApiTransport(active_provider, source.api_transport ?? active_providerConfig.api_transport),
    },
  };
}

function looksLikeDaemonAgentConfig(value: unknown): value is DiskAgentSettings {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Boolean(
    typeof record.provider === "string"
    || typeof record.active_provider === "string"
    || (record.providers && typeof record.providers === "object" && !Array.isArray(record.providers))
    || typeof record.agent_backend === "string",
  );
}

function isValidConciergeConfig(value: unknown): value is AgentState["conciergeConfig"] {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    typeof record.enabled === "boolean"
    || typeof record.detail_level === "string"
    || typeof record.auto_cleanup_on_navigate === "boolean"
  );
}

function isValidProviderAuthStates(value: unknown): value is ProviderAuthState[] {
  return Array.isArray(value)
    && value.length > 0
    && value.every((entry) =>
      entry
      && typeof entry === "object"
      && typeof (entry as ProviderAuthState).provider_id === "string"
      && typeof (entry as ProviderAuthState).provider_name === "string");
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
      api_transport: message.api_transport ?? null,
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
    agent_name: thread.agent_name ?? "assistant",
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
    api_transport: typeof metadata.api_transport === "string"
      ? normalizeApiTransport(
        typeof message.provider === "string"
          ? normalizeAgentProviderId(message.provider)
          : "openai",
        metadata.api_transport,
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
  agentSettingsHydrated: false,
  agentSettingsDirty: false,
  searchQuery: "",
  providerAuthStates: [],
  subAgents: [],
  refreshProviderAuthStates: async () => {
    const bridge = getBridge();
    if (!bridge?.agentGetProviderAuthStates) return;
    try {
      const states = await bridge.agentGetProviderAuthStates();
      if (isValidProviderAuthStates(states)) {
        set({ providerAuthStates: states as ProviderAuthState[] });
      }
    } catch { /* ignore */ }
  },
  validateProvider: async (providerId, base_url, api_key, auth_source) => {
    const bridge = getBridge();
    if (!bridge?.agentValidateProvider) return { valid: false, error: "Bridge not available" };
    try {
      return await bridge.agentValidateProvider(providerId, base_url, api_key, auth_source);
    } catch (e) {
      return { valid: false, error: String(e) };
    }
  },
  loginProvider: async (providerId, api_key, base_url) => {
    const bridge = getBridge();
    if (!bridge?.agentLoginProvider) return;
    try {
      const result = await bridge.agentLoginProvider(providerId, api_key, base_url);
      // The daemon returns updated auth states directly.
      if (isValidProviderAuthStates(result)) {
        set({ providerAuthStates: result as ProviderAuthState[] });
      }
    } catch { /* ignore */ }
  },
  logoutProvider: async (providerId) => {
    const bridge = getBridge();
    if (!bridge?.agentLogoutProvider) return;
    try {
      const result = await bridge.agentLogoutProvider(providerId);
      if (isValidProviderAuthStates(result)) {
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
      agent_name: get().agentSettings.agent_name,
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
      if (shouldPersistHistory(get().agentSettings.agent_backend)) {
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
      if (shouldPersistHistory(get().agentSettings.agent_backend)) {
        persistDaemonThreadMap(next.threads);
        void getAgentDbApi()?.dbDeleteThread?.(id);
      }
      return next;
    });
  },

  setActiveThread: (id) => {
    set({ activeThreadId: id });
    if (shouldPersistHistory(get().agentSettings.agent_backend)) {
      scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
    }
  },

  searchThreads: (query) => {
    const lower = query.toLowerCase();
    return get().threads.filter(
      (t) =>
        t.title.toLowerCase().includes(lower) ||
        t.lastMessagePreview.toLowerCase().includes(lower) ||
        t.agent_name.toLowerCase().includes(lower)
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
      if (shouldPersistHistory(get().agentSettings.agent_backend)) {
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
        api_transport: meta?.api_transport ?? last.api_transport,
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
      if (shouldPersistHistory(get().agentSettings.agent_backend)) {
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

  deleteMessage: (threadId, messageId) => {
    set((s) => {
      const msgs = s.messages[threadId];
      if (!msgs) return s;
      const filtered = msgs.filter((m) => m.id !== messageId);
      if (filtered.length === msgs.length) return s; // not found
      return {
        messages: { ...s.messages, [threadId]: filtered },
        threads: s.threads.map((t) =>
          t.id === threadId
            ? { ...t, messageCount: Math.max(0, t.messageCount - 1), updatedAt: Date.now() }
            : t
        ),
      };
    });
    // Persist deletion to daemon
    const api = getAgentDbApi();
    api?.dbDeleteMessage?.(threadId, messageId);
  },

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
      if (shouldPersistHistory(get().agentSettings.agent_backend)) {
        persistDaemonThreadMap(threads);
      }
      return { threads };
    });
  },

  toggleAgentPanel: () => set((s) => ({ agentPanelOpen: !s.agentPanelOpen })),
  setSearchQuery: (q) => set({ searchQuery: q }),

  updateAgentSetting: (key, value) => {
    set((s) => {
      const nextValue = key === "active_provider"
        ? normalizeAgentProviderId(value)
        : value;
      const updated = { ...s.agentSettings, [key]: nextValue };
      return {
        agentSettings: updated,
        agentSettingsDirty: s.agentSettingsHydrated,
      };
    });
  },

  resetAgentSettings: () => {
    const def = { ...DEFAULT_AGENT_SETTINGS };
    set((s) => ({
      agentSettings: def,
      agentSettingsDirty: s.agentSettingsHydrated,
    }));
  },

  refreshAgentSettingsFromDaemon: async () => {
    const bridge = getBridge();
    if (!bridge?.agentGetConfig) {
      set({ agentSettingsHydrated: true, agentSettingsDirty: false });
      return true;
    }

    try {
      const daemonState = await bridge.agentGetConfig();
      if (!looksLikeDaemonAgentConfig(daemonState)) {
        set({ agentSettingsHydrated: false });
        return false;
      }
      const merged = normalizeAgentSettingsFromSource(daemonState as DiskAgentSettings);
      set({
        agentSettings: merged,
        agentSettingsHydrated: true,
        agentSettingsDirty: false,
      });
      return true;
    } catch {
      set({ agentSettingsHydrated: false });
      return false;
    }
  },

  markAgentSettingsSynced: () => set({ agentSettingsDirty: false }),

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
      if (isValidConciergeConfig(config)) {
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
        if (isValidConciergeConfig(refreshed)) {
          set({ conciergeConfig: refreshed as any });
          return;
        }
      }
      if (isValidConciergeConfig(config)) {
        set({ conciergeConfig: config as any });
      }
    } catch { /* ignore */ }
  },
  dismissConciergeWelcome: async () => {
    const bridge = getBridge();
    if (!bridge?.agentDismissConciergeWelcome) return;
    try {
      await bridge.agentDismissConciergeWelcome();
      set({ conciergeWelcome: null });
      if (bridge.agentGetThread) {
        const remoteThread = await bridge.agentGetThread("concierge").catch(() => null);
        const hydrated = buildHydratedRemoteThread(
          (remoteThread ?? {}) as RemoteAgentThreadRecord,
          get().agentSettings.agent_name,
        );
        if (hydrated) {
          set((state) => {
            const existing = state.threads.find((thread) => thread.daemonThreadId === "concierge");
            if (!existing) {
              return state;
            }
            return {
              threads: state.threads.map((thread) => thread.id === existing.id ? {
                ...hydrated.thread,
                id: existing.id,
              } : thread),
              messages: {
                ...state.messages,
                [existing.id]: hydrated.messages.map((message) => ({
                  ...message,
                  threadId: existing.id,
                })),
              },
            };
          });
        }
      }
    } catch { /* ignore */ }
  },
  gatewayStatuses: {},
  setGatewayStatus: (platform, status, lastError, consecutiveFailures) => {
    set((state) => ({
      gatewayStatuses: {
        ...state.gatewayStatuses,
        [platform]: {
          status,
          lastError,
          consecutiveFailures,
          updatedAt: Date.now(),
        },
      },
    }));
  },
  getThreadsForPane: (paneId) => get().threads.filter((t) => t.paneId === paneId),
}));

export async function hydrateAgentStore(): Promise<void> {
  const bridge = getBridge();
  let configuredBackend = DEFAULT_AGENT_SETTINGS.agent_backend;
  let agentSettingsHydrated = false;

  if (bridge?.agentGetConfig) {
    const daemonState = await bridge.agentGetConfig().catch(() => null);
    if (looksLikeDaemonAgentConfig(daemonState)) {
      const merged = normalizeAgentSettingsFromSource(daemonState as DiskAgentSettings);
      configuredBackend = merged.agent_backend;
      useAgentStore.setState({
        agentSettings: merged,
        agentSettingsDirty: false,
      });
      agentSettingsHydrated = true;
    }
  } else {
    agentSettingsHydrated = true;
  }

  useAgentStore.setState({ agentSettingsHydrated });

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
            useAgentStore.getState().agentSettings.agent_name,
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
