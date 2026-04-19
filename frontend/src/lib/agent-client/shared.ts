import type { AgentProviderId } from "../agentStore";
import { getProviderDefinition } from "../agentStore";
import type { ApiChatMessage, ChatRequest } from "./types";

const OPENROUTER_ATTRIBUTION_URL = "https://tamux.app";
const OPENROUTER_ATTRIBUTION_TITLE = "tamux";
const OPENROUTER_ATTRIBUTION_CATEGORIES = "cli-agent";

export class TransportCompatibilityError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportCompatibilityError";
  }
}

export function isDashScopeCodingPlanAnthropicBaseUrl(baseUrl: string): boolean {
  const lower = (baseUrl || "").trim().toLowerCase();
  return lower.includes("dashscope.aliyuncs.com") && lower.includes("/apps/anthropic");
}

export function usesDashScopeEnableThinking(
  provider: AgentProviderId,
  model: string,
): boolean {
  return (provider === "qwen" || provider === "alibaba-coding-plan") &&
    [
      "qwen-max",
      "qwen-plus",
      "qwen3.6-plus",
      "qwen3.5-plus",
      "qwen3-max-2026-01-23",
      "glm-4.7",
      "glm-5",
    ].includes(model);
}

export function applyDashScopeCodingPlanHeaders(
  provider: AgentProviderId,
  baseUrl: string,
  apiType: "openai" | "anthropic",
  headers: Record<string, string>,
): void {
  if (provider !== "alibaba-coding-plan") return;
  headers["User-Agent"] =
    apiType === "anthropic" ? "Anthropic/JS tamux" : "OpenAI/JS tamux";
  if (apiType === "openai" && !isDashScopeCodingPlanAnthropicBaseUrl(baseUrl)) {
    headers["x-stainless-lang"] = "js";
    headers["x-stainless-package-version"] = "tamux";
  }
}

export function applyOpenRouterAttributionHeaders(
  provider: AgentProviderId,
  headers: Record<string, string>,
): void {
  if (provider !== "openrouter") return;
  headers["HTTP-Referer"] = OPENROUTER_ATTRIBUTION_URL;
  headers["X-OpenRouter-Title"] = OPENROUTER_ATTRIBUTION_TITLE;
  headers["X-OpenRouter-Categories"] = OPENROUTER_ATTRIBUTION_CATEGORIES;
}

export function buildChatCompletionUrl(
  provider: AgentProviderId,
  base_url: string,
): string {
  const base = base_url.replace(/\/$/, "");
  const lowerBase = base.toLowerCase();

  if (provider === "openrouter" || provider === "groq") {
    return `${base}/chat/completions`;
  }
  if (/(^|\/)api\/v1$/.test(lowerBase) || /(^|\/)v1$/.test(lowerBase)) {
    return `${base}/chat/completions`;
  }
  return `${base}/v1/chat/completions`;
}

export function buildResponsesUrl(base_url: string): string {
  const base = base_url.replace(/\/$/, "");
  const lowerBase = base.toLowerCase();

  if (
    /(^|\/)api\/v1$/.test(lowerBase) ||
    /(^|\/)v[1-4]$/.test(lowerBase) ||
    /(^|\/)openai\/v1$/.test(lowerBase) ||
    /(^|\/)compatible-mode\/v1$/.test(lowerBase)
  ) {
    return `${base}/responses`;
  }

  return `${base}/v1/responses`;
}

export function buildNativeAssistantBaseUrl(
  provider: AgentProviderId,
  base_url: string,
): string {
  const providerBase = getProviderDefinition(provider)?.nativeBaseUrl;
  return (providerBase || base_url).replace(/\/$/, "");
}

export function messageContentToText(message: ApiChatMessage): string {
  return typeof message.content === "string" ? message.content : "";
}

export function isChatGptSubscriptionRequest(req: ChatRequest): boolean {
  return req.provider === "openai" && req.config.auth_source === "chatgpt_subscription";
}

export function buildChatGptCodexResponsesUrl(): string {
  return "https://chatgpt.com/backend-api/codex/responses";
}

export function buildChatGptCodexHeaders(
  api_key: string,
  accountId?: string,
  upstreamThreadId?: string,
): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    Authorization: `Bearer ${api_key}`,
    "OpenAI-Beta": "responses=experimental",
    originator: "tamux",
  };
  if (accountId) {
    headers["chatgpt-account-id"] = accountId;
  }
  const threadId = upstreamThreadId?.trim();
  if (threadId) {
    headers["session_id"] = threadId;
    headers["x-client-request-id"] = threadId;
  }
  return headers;
}
