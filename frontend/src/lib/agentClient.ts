/**
 * LLM API client for agent chat.
 *
 * Supports two API formats:
 *  - OpenAI-compatible (most providers)
 *  - Anthropic Messages API
 *
 * All providers are called directly from the frontend via fetch().
 */

import type {
  AgentProviderId,
  AgentProviderConfig,
  AgentMessage,
  AgentThread,
  ApiTransportMode,
} from "./agentStore";
import {
  getDefaultApiTransport,
  getProviderApiType,
  getProviderDefinition,
  getSupportedApiTransports,
  providerSupportsResponseContinuity,
} from "./agentStore";
import type { ToolDefinition, ToolCall } from "./agentTools";

const APPROX_CHARS_PER_TOKEN = 4;
const MIN_CONTEXT_TARGET_TOKENS = 1024;

export interface ApiChatMessage {
  role: string;
  content: string;
  tool_call_id?: string;
  name?: string;
  tool_calls?: ToolCall[];
}

export interface ChatRequest {
  provider: AgentProviderId;
  config: AgentProviderConfig;
  systemPrompt: string;
  messages: ApiChatMessage[];
  streaming: boolean;
  signal?: AbortSignal;
  tools?: ToolDefinition[];
  reasoningEffort?: string;
  previousResponseId?: string;
  upstreamThreadId?: string;
}

export interface ChatChunk {
  type: "delta" | "done" | "error" | "tool_calls" | "transport_fallback";
  content: string;
  reasoning?: string;
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  cost?: number;
  reasoningTokens?: number;
  audioTokens?: number;
  videoTokens?: number;
  toolCalls?: ToolCall[];
  responseId?: string;
  upstreamThreadId?: string;
  fromTransport?: ApiTransportMode;
  toTransport?: ApiTransportMode;
}

export interface PreparedOpenAIRequest {
  messages: ApiChatMessage[];
  transport: ApiTransportMode;
  previousResponseId?: string;
  upstreamThreadId?: string;
}

type OpenAICodexAuthStatus = {
  available?: boolean;
  authMode?: string;
  accountId?: string;
  expiresAt?: number;
  source?: string;
  apiKey?: string;
  error?: string;
};

type ResolvedProviderAuth = {
  apiKey: string;
  accountId?: string;
};

class TransportCompatibilityError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportCompatibilityError";
  }
}

export interface ContextCompactionSettings {
  autoCompactContext: boolean;
  maxContextMessages: number;
  contextWindowTokens: number;
  contextBudgetTokens: number;
  compactThresholdPercent: number;
  keepRecentOnCompaction: number;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Send a chat completion request. Returns an async iterator of content chunks
 * for streaming, or a single chunk for non-streaming.
 */
export async function* sendChatCompletion(
  req: ChatRequest,
): AsyncGenerator<ChatChunk> {
  const resolvedAuth = await resolveProviderAuth(req);
  const resolvedRequest: ChatRequest = {
    ...req,
    config: {
      ...req.config,
      apiKey: resolvedAuth.apiKey,
    },
  };

  if (!resolvedRequest.config.apiKey && resolvedRequest.provider !== "ollama") {
    yield { type: "error", content: `No API key configured for ${req.provider}. Open Settings > Agent to add your key.` };
    return;
  }

  if (!resolvedRequest.config.baseUrl) {
    yield { type: "error", content: `No base URL configured for ${req.provider}.` };
    return;
  }

  try {
    const supportedTransports = getSupportedApiTransports(resolvedRequest.provider);
    const selectedTransport = supportedTransports.includes(resolvedRequest.config.apiTransport)
      ? resolvedRequest.config.apiTransport
      : getDefaultApiTransport(resolvedRequest.provider);

    if (getProviderApiType(resolvedRequest.provider, resolvedRequest.config.model) === "anthropic") {
      yield* sendAnthropic(resolvedRequest);
    } else if (selectedTransport === "native_assistant") {
      try {
        yield* sendNativeAssistant({
          ...resolvedRequest,
          config: { ...resolvedRequest.config, apiTransport: "native_assistant" },
        });
      } catch (err: any) {
        if (err instanceof TransportCompatibilityError) {
          yield {
            type: "transport_fallback",
            content: err.message,
            fromTransport: "native_assistant",
            toTransport: "chat_completions",
          };
          yield* sendOpenAICompatible({
            ...resolvedRequest,
            config: { ...resolvedRequest.config, apiTransport: "chat_completions" },
            previousResponseId: undefined,
            upstreamThreadId: undefined,
          });
        } else {
          throw err;
        }
      }
    } else if (selectedTransport === "responses") {
      try {
        yield* sendOpenAIResponses({
          ...resolvedRequest,
          config: { ...resolvedRequest.config, apiTransport: "responses" },
          _chatgptAccountId: resolvedAuth.accountId,
        });
      } catch (err: any) {
        if (err instanceof TransportCompatibilityError) {
          yield {
            type: "transport_fallback",
            content: err.message,
            fromTransport: "responses",
            toTransport: "chat_completions",
          };
          yield* sendOpenAICompatible({
            ...resolvedRequest,
            config: { ...resolvedRequest.config, apiTransport: "chat_completions" },
            previousResponseId: undefined,
            upstreamThreadId: undefined,
          });
        } else {
          throw err;
        }
      }
    } else {
      yield* sendOpenAICompatible(resolvedRequest, resolvedAuth.accountId);
    }
  } catch (err: any) {
    if (err.name === "AbortError") {
      yield { type: "done", content: "" };
    } else {
      yield { type: "error", content: `API error: ${err.message || String(err)}` };
    }
  }
}

async function resolveProviderAuth(req: ChatRequest): Promise<ResolvedProviderAuth> {
  if (req.provider !== "openai" || req.config.authSource !== "chatgpt_subscription") {
    return { apiKey: req.config.apiKey };
  }

  const amux = (window as any).amux || (window as any).tamux;
  if (!amux?.openAICodexAuthStatus) {
    throw new Error("ChatGPT subscription auth is unavailable in this build.");
  }

  const status = await amux.openAICodexAuthStatus({ refresh: true }) as OpenAICodexAuthStatus;
  if (status?.available && typeof status.apiKey === "string" && status.apiKey.trim()) {
    return {
      apiKey: status.apiKey.trim(),
      accountId: typeof status.accountId === "string" ? status.accountId.trim() : undefined,
    };
  }

  throw new Error(status?.error || "ChatGPT subscription auth not found. Authenticate in Settings > Agent.");
}

// ---------------------------------------------------------------------------
// OpenAI-compatible API (covers 15 of 16 providers)
// ---------------------------------------------------------------------------

function buildChatCompletionUrl(provider: AgentProviderId, baseUrl: string): string {
  const base = baseUrl.replace(/\/$/, "");
  const lowerBase = base.toLowerCase();

  // OpenRouter/Groq defaults already include /api/v1 or /openai/v1.
  if (provider === "openrouter" || provider === "groq") {
    return `${base}/chat/completions`;
  }

  // Most other providers in this app use base URLs ending with /v1.
  if (/(^|\/)api\/v1$/.test(lowerBase) || /(^|\/)v1$/.test(lowerBase)) {
    return `${base}/chat/completions`;
  }

  // For custom/unversioned endpoints, use explicit /v1 path.
  return `${base}/v1/chat/completions`;
}

function buildResponsesUrl(baseUrl: string): string {
  const base = baseUrl.replace(/\/$/, "");
  const lowerBase = base.toLowerCase();

  if (/(^|\/)api\/v1$/.test(lowerBase) || /(^|\/)v[1-4]$/.test(lowerBase) || /(^|\/)openai\/v1$/.test(lowerBase) || /(^|\/)compatible-mode\/v1$/.test(lowerBase)) {
    return `${base}/responses`;
  }

  return `${base}/v1/responses`;
}

function buildNativeAssistantBaseUrl(provider: AgentProviderId, baseUrl: string): string {
  const providerBase = getProviderDefinition(provider)?.nativeBaseUrl;
  return (providerBase || baseUrl).replace(/\/$/, "");
}

function messageContentToText(message: ApiChatMessage): string {
  return typeof message.content === "string" ? message.content : "";
}

async function* sendNativeAssistant(req: ChatRequest): AsyncGenerator<ChatChunk> {
  const providerDef = getProviderDefinition(req.provider);
  if (!providerDef?.nativeTransportKind) {
    throw new TransportCompatibilityError(
      `${req.provider} does not expose a native assistant API`,
    );
  }
  if (!req.config.assistantId.trim()) {
    throw new TransportCompatibilityError(
      `${req.provider} native assistant requires Assistant ID`,
    );
  }

  const baseUrl = buildNativeAssistantBaseUrl(req.provider, req.config.baseUrl);
  const latestUserMessage = [...req.messages]
    .reverse()
    .find((message) => message.role === "user");
  const userText = latestUserMessage ? messageContentToText(latestUserMessage).trim() : "";
  if (!userText) {
    throw new Error("native assistant requires a user message");
  }

  const authHeaders: HeadersInit = {
    "Content-Type": "application/json",
    ...(req.config.apiKey ? { Authorization: `Bearer ${req.config.apiKey}` } : {}),
  };
  const compatibilityStatuses = new Set([400, 404, 405, 422]);

  let threadId = req.upstreamThreadId?.trim();
  if (!threadId) {
    const threadResponse = await fetch(`${baseUrl}/threads`, {
      method: "POST",
      headers: authHeaders,
      body: "{}",
      signal: req.signal,
    });
    if (!threadResponse.ok) {
      const text = await threadResponse.text().catch(() => "");
      if (compatibilityStatuses.has(threadResponse.status)) {
        throw new TransportCompatibilityError(
          `Native assistant thread creation failed (${threadResponse.status}): ${text.slice(0, 240)}`,
        );
      }
      yield { type: "error", content: `${req.provider} API returned ${threadResponse.status}: ${text.slice(0, 200)}` };
      return;
    }
    const threadJson = await threadResponse.json();
    threadId = typeof threadJson?.id === "string" ? threadJson.id : "";
    if (!threadId) {
      yield { type: "error", content: `${req.provider} native assistant returned no thread id.` };
      return;
    }
  }

  const messageResponse = await fetch(`${baseUrl}/threads/${threadId}/messages`, {
    method: "POST",
    headers: authHeaders,
    body: JSON.stringify({ role: "user", content: userText }),
    signal: req.signal,
  });
  if (!messageResponse.ok) {
    const text = await messageResponse.text().catch(() => "");
    if (compatibilityStatuses.has(messageResponse.status)) {
      throw new TransportCompatibilityError(
        `Native assistant message append failed (${messageResponse.status}): ${text.slice(0, 240)}`,
      );
    }
    yield { type: "error", content: `${req.provider} API returned ${messageResponse.status}: ${text.slice(0, 200)}` };
    return;
  }

  const runResponse = await fetch(`${baseUrl}/threads/${threadId}/runs`, {
    method: "POST",
    headers: authHeaders,
    body: JSON.stringify({ assistant_id: req.config.assistantId }),
    signal: req.signal,
  });
  if (!runResponse.ok) {
    const text = await runResponse.text().catch(() => "");
    if (compatibilityStatuses.has(runResponse.status)) {
      throw new TransportCompatibilityError(
        `Native assistant run creation failed (${runResponse.status}): ${text.slice(0, 240)}`,
      );
    }
    yield { type: "error", content: `${req.provider} API returned ${runResponse.status}: ${text.slice(0, 200)}` };
    return;
  }
  const runJson = await runResponse.json();
  const runId = typeof runJson?.id === "string" ? runJson.id : "";
  if (!runId) {
    yield { type: "error", content: `${req.provider} native assistant returned no run id.` };
    return;
  }

  let inputTokens = 0;
  let outputTokens = 0;
  for (let attempt = 0; attempt < 180; attempt += 1) {
    if (req.signal?.aborted) {
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 1000));
    const statusResponse = await fetch(`${baseUrl}/threads/${threadId}/runs/${runId}`, {
      method: "GET",
      headers: authHeaders,
      signal: req.signal,
    });
    if (!statusResponse.ok) {
      const text = await statusResponse.text().catch(() => "");
      yield { type: "error", content: `${req.provider} API returned ${statusResponse.status}: ${text.slice(0, 200)}` };
      return;
    }
    const statusJson = await statusResponse.json();
    inputTokens = Number(statusJson?.usage?.prompt_tokens ?? statusJson?.usage?.input_tokens ?? inputTokens);
    outputTokens = Number(statusJson?.usage?.completion_tokens ?? statusJson?.usage?.output_tokens ?? outputTokens);
    switch (statusJson?.status) {
      case "queued":
      case "in_progress":
        continue;
      case "completed": {
        const messagesResponse = await fetch(`${baseUrl}/threads/${threadId}/messages?order=desc&limit=20`, {
          method: "GET",
          headers: authHeaders,
          signal: req.signal,
        });
        if (!messagesResponse.ok) {
          const text = await messagesResponse.text().catch(() => "");
          yield { type: "error", content: `${req.provider} API returned ${messagesResponse.status}: ${text.slice(0, 200)}` };
          return;
        }
        const messagesJson = await messagesResponse.json();
        const data = Array.isArray(messagesJson?.data) ? messagesJson.data : [];
        const assistantMessage = data.find((message: any) => message?.role === "assistant");
        const content = Array.isArray(assistantMessage?.content)
          ? assistantMessage.content
            .map((part: any) => {
              if (typeof part?.text?.value === "string") return part.text.value;
              if (typeof part?.text === "string") return part.text;
              return "";
            })
            .filter(Boolean)
            .join("\n")
          : typeof assistantMessage?.content === "string"
            ? assistantMessage.content
            : "";
        yield {
          type: "done",
          content,
          inputTokens,
          outputTokens,
          upstreamThreadId: threadId,
        };
        return;
      }
      case "requires_action":
        yield { type: "error", content: `${req.provider} native assistant requested external tool action, which is not proxied in legacy mode.` };
        return;
      case "failed":
      case "cancelled":
      case "expired":
        yield {
          type: "error",
          content: statusJson?.last_error?.message || `${req.provider} native assistant run failed.`,
        };
        return;
      default:
        yield { type: "error", content: `${req.provider} native assistant entered unexpected status '${String(statusJson?.status ?? "")}'.` };
        return;
    }
  }

  yield { type: "error", content: `${req.provider} native assistant timed out waiting for completion.` };
}

function isChatGptSubscriptionRequest(req: ChatRequest): boolean {
  return req.provider === "openai" && req.config.authSource === "chatgpt_subscription";
}

function buildChatGptCodexResponsesUrl(): string {
  return "https://chatgpt.com/backend-api/codex/responses";
}

function buildChatGptCodexHeaders(apiKey: string, accountId?: string): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    "Authorization": `Bearer ${apiKey}`,
    "OpenAI-Beta": "responses=experimental",
    "originator": "tamux",
  };
  if (accountId) {
    headers["chatgpt-account-id"] = accountId;
  }
  return headers;
}

async function* sendOpenAICompatible(
  req: ChatRequest,
  chatgptAccountId?: string,
): AsyncGenerator<ChatChunk> {
  if (isChatGptSubscriptionRequest(req)) {
    yield* sendOpenAIResponses({
      ...req,
      config: {
        ...req.config,
        apiTransport: "responses",
      },
      _chatgptAccountId: chatgptAccountId,
    });
    return;
  }

  const url = buildChatCompletionUrl(req.provider, req.config.baseUrl);

  const body: Record<string, unknown> = {
    model: req.config.model,
    messages: [
      { role: "system", content: req.systemPrompt },
      ...req.messages,
    ],
    stream: req.streaming,
  };

  // Include tool definitions if provided
  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools;
    body.tool_choice = "auto";
  }

  // Add reasoning_effort for OpenAI-compatible reasoning models
  if (req.reasoningEffort && req.reasoningEffort !== "none") {
    body.reasoning_effort = req.reasoningEffort === "xhigh" ? "high" : req.reasoningEffort;
    body.reasoning = { effort: req.reasoningEffort };
  }

  // Request usage details including reasoning tokens
  if (req.streaming) {
    body.stream_options = { include_usage: true };
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (req.config.apiKey) {
    headers["Authorization"] = `Bearer ${req.config.apiKey}`;
  }

  const response = await fetch(url, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: req.signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    yield { type: "error", content: `${req.provider} API returned ${response.status}: ${text.slice(0, 200)}` };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseSSEStream(response.body, req.signal);
  } else {
    const json = await response.json();
    const msg = json.choices?.[0]?.message;
    const content = msg?.content ?? "";

    // Check for tool calls
    if (msg?.tool_calls && msg.tool_calls.length > 0) {
      yield {
        type: "tool_calls",
        content: content,
        toolCalls: msg.tool_calls.map((tc: any) => ({
          id: tc.id,
          type: "function",
          function: {
            name: tc.function.name,
            arguments: tc.function.arguments,
          },
        })),
        inputTokens: json.usage?.prompt_tokens ?? 0,
        outputTokens: json.usage?.completion_tokens ?? 0,
      };
    } else {
      yield {
        type: "done",
        content,
        inputTokens: json.usage?.prompt_tokens ?? 0,
        outputTokens: json.usage?.completion_tokens ?? 0,
      };
    }
  }
}

async function* sendOpenAIResponses(
  req: ChatRequest & { _chatgptAccountId?: string },
): AsyncGenerator<ChatChunk> {
  const isSubscription = isChatGptSubscriptionRequest(req);
  const url = isSubscription
    ? buildChatGptCodexResponsesUrl()
    : buildResponsesUrl(req.config.baseUrl);
  const body: Record<string, unknown> = {
    model: req.config.model,
    instructions: req.systemPrompt,
    input: req.messages.map((message) => {
      if (message.role === "tool") {
        return {
          type: "function_call_output",
          call_id: message.tool_call_id,
          output: message.content,
        };
      }
      return {
        role: message.role,
        content: message.content,
      };
    }),
    stream: req.streaming,
    ...(isSubscription ? { store: false } : {}),
  };

  if (req.previousResponseId) {
    body.previous_response_id = req.previousResponseId;
  }

  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools.map((tool) => ({
      type: tool.type,
      name: tool.function.name,
      description: tool.function.description,
      parameters: tool.function.parameters,
    }));
  }

  if (req.reasoningEffort && req.reasoningEffort !== "none") {
    body.reasoning = {
      effort: req.reasoningEffort === "xhigh" ? "high" : req.reasoningEffort,
    };
  }

  if (isSubscription) {
    body.include = ["reasoning.encrypted_content"];
    body.text = {
      ...(typeof body.text === "object" && body.text !== null ? body.text as Record<string, unknown> : {}),
      verbosity: "high",
    };
  }

  const response = await fetch(url, {
    method: "POST",
    headers: isSubscription
      ? buildChatGptCodexHeaders(req.config.apiKey, req._chatgptAccountId)
      : {
          "Content-Type": "application/json",
          ...(req.config.apiKey ? { Authorization: `Bearer ${req.config.apiKey}` } : {}),
        },
    body: JSON.stringify(body),
    signal: req.signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    if ([400, 404, 405, 415, 422].includes(response.status)) {
      throw new TransportCompatibilityError(
        `Responses API rejected the request (${response.status}): ${text.slice(0, 240)}`,
      );
    }
    yield { type: "error", content: `${req.provider} API returned ${response.status}: ${text.slice(0, 200)}` };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseResponsesSSE(response.body, req.provider, req.signal);
  } else {
    const json = await response.json();
    const responseId = typeof json?.id === "string" ? json.id : undefined;
    const output = Array.isArray(json?.output) ? json.output : [];
    const outputText = output
      .filter((item: any) => item?.type === "message")
      .flatMap((item: any) => Array.isArray(item?.content) ? item.content : [])
      .filter((part: any) => typeof part?.text === "string")
      .map((part: any) => part.text)
      .join("");
    const functionCalls = output
      .filter((item: any) => item?.type === "function_call")
      .map((item: any) => ({
        id: item.call_id ?? item.id,
        type: "function" as const,
        function: {
          name: item.name,
          arguments: item.arguments ?? "",
        },
      }));

    if (functionCalls.length > 0) {
      yield {
        type: "tool_calls",
        content: outputText,
        toolCalls: functionCalls,
        inputTokens: json?.usage?.input_tokens ?? 0,
        outputTokens: json?.usage?.output_tokens ?? 0,
        responseId,
      };
      return;
    }

    yield {
      type: "done",
      content: outputText,
      inputTokens: json?.usage?.input_tokens ?? 0,
      outputTokens: json?.usage?.output_tokens ?? 0,
      responseId,
    };
  }
}

// ---------------------------------------------------------------------------
// Anthropic Messages API
// ---------------------------------------------------------------------------

async function* sendAnthropic(req: ChatRequest): AsyncGenerator<ChatChunk> {
  const url = `${req.config.baseUrl.replace(/\/$/, "")}/v1/messages`;

  const anthropicMessages = req.messages.map((m) => {
    const role = m.role === "system" ? "user" : (m.role === "tool" ? "user" : m.role);

    if (m.role === "tool") {
      return {
        role,
        content: [{ type: "tool_result", tool_use_id: m.tool_call_id, content: m.content }],
      };
    }

    if (m.role === "assistant" && m.tool_calls && m.tool_calls.length > 0) {
      const blocks: Array<Record<string, unknown>> = [];
      if (m.content) {
        blocks.push({ type: "text", text: m.content });
      }
      for (const toolCall of m.tool_calls) {
        let parsedArguments: unknown = {};
        try {
          parsedArguments = JSON.parse(toolCall.function.arguments || "{}");
        } catch {
          parsedArguments = { _raw_arguments: toolCall.function.arguments || "" };
        }
        blocks.push({
          type: "tool_use",
          id: toolCall.id,
          name: toolCall.function.name,
          input: parsedArguments,
        });
      }
      return { role, content: blocks };
    }

    return { role, content: m.content };
  });

  const body: Record<string, unknown> = {
    model: req.config.model,
    max_tokens: 4096,
    system: req.systemPrompt,
    messages: anthropicMessages,
    stream: req.streaming,
  };

  // Include tool definitions in Anthropic format
  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools.map((t) => ({
      name: t.function.name,
      description: t.function.description,
      input_schema: t.function.parameters,
    }));
  }

  // Add extended thinking for Anthropic models
  if (req.reasoningEffort && req.reasoningEffort !== "none") {
    const budgetMap: Record<string, number> = { minimal: 512, low: 1024, medium: 4096, high: 8192, xhigh: 16384 };
    const budgetTokens = budgetMap[req.reasoningEffort] ?? 4096;
    body.thinking = { type: "enabled", budget_tokens: budgetTokens };
    // Increase max_tokens when thinking is enabled
    body.max_tokens = Math.max(4096, budgetTokens + 4096);
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    "x-api-key": req.config.apiKey,
    "anthropic-version": "2023-06-01",
    "anthropic-dangerous-direct-browser-access": "true",
  };

  const response = await fetch(url, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: req.signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    yield { type: "error", content: `Anthropic API returned ${response.status}: ${text.slice(0, 200)}` };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseAnthropicSSE(response.body, req.signal);
  } else {
    const json = await response.json();

    // Check for tool_use blocks in Anthropic response
    const toolUseBlocks = json.content?.filter((b: any) => b.type === "tool_use") ?? [];
    if (toolUseBlocks.length > 0) {
      const textContent = json.content?.filter((b: any) => b.type === "text").map((b: any) => b.text).join("") ?? "";
      yield {
        type: "tool_calls",
        content: textContent,
        toolCalls: toolUseBlocks.map((b: any) => ({
          id: b.id,
          type: "function" as const,
          function: {
            name: b.name,
            arguments: JSON.stringify(b.input),
          },
        })),
        inputTokens: json.usage?.input_tokens ?? 0,
        outputTokens: json.usage?.output_tokens ?? 0,
      };
    } else {
      const content = json.content?.[0]?.text ?? "";
      yield {
        type: "done",
        content,
        inputTokens: json.usage?.input_tokens ?? 0,
        outputTokens: json.usage?.output_tokens ?? 0,
      };
    }
  }
}

// ---------------------------------------------------------------------------
// SSE parsers
// ---------------------------------------------------------------------------

async function* parseSSEStream(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal,
): AsyncGenerator<ChatChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let totalContent = "";
  let totalReasoning = "";
  const pendingToolCalls: Map<number, { id: string; name: string; args: string }> = new Map();
  let usage: {
    inputTokens?: number;
    outputTokens?: number;
    totalTokens?: number;
    cost?: number;
    reasoningTokens?: number;
    audioTokens?: number;
    videoTokens?: number;
  } = {};

  try {
    while (true) {
      if (signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (!line.startsWith("data: ")) continue;
        const data = line.slice(6).trim();
        if (data === "[DONE]") {
          // If we accumulated tool calls, yield them
          if (pendingToolCalls.size > 0) {
            yield {
              type: "tool_calls",
              content: totalContent,
              reasoning: totalReasoning,
              toolCalls: Array.from(pendingToolCalls.values()).map((tc) => ({
                id: tc.id,
                type: "function" as const,
                function: { name: tc.name, arguments: tc.args },
              })),
              inputTokens: usage.inputTokens,
              outputTokens: usage.outputTokens,
              totalTokens: usage.totalTokens,
              cost: usage.cost,
              reasoningTokens: usage.reasoningTokens,
              audioTokens: usage.audioTokens,
              videoTokens: usage.videoTokens,
            };
          } else {
            yield {
              type: "done",
              content: totalContent,
              reasoning: totalReasoning,
              inputTokens: usage.inputTokens,
              outputTokens: usage.outputTokens,
              totalTokens: usage.totalTokens,
              cost: usage.cost,
              reasoningTokens: usage.reasoningTokens,
              audioTokens: usage.audioTokens,
              videoTokens: usage.videoTokens,
            };
          }
          return;
        }

        try {
          const parsed = JSON.parse(data);
          const delta = parsed.choices?.[0]?.delta;

          if (parsed.usage) {
            const parsedUsage = parsed.usage;
            usage = {
              inputTokens: Number(parsedUsage.prompt_tokens ?? usage.inputTokens ?? 0),
              outputTokens: Number(parsedUsage.completion_tokens ?? usage.outputTokens ?? 0),
              totalTokens: Number(parsedUsage.total_tokens ?? usage.totalTokens ?? 0),
              cost: parsedUsage.cost !== undefined ? Number(parsedUsage.cost) : usage.cost,
              reasoningTokens:
                parsedUsage.completion_tokens_details?.reasoning_tokens !== undefined
                  ? Number(parsedUsage.completion_tokens_details.reasoning_tokens)
                  : usage.reasoningTokens,
              audioTokens:
                parsedUsage.completion_tokens_details?.audio_tokens !== undefined
                  ? Number(parsedUsage.completion_tokens_details.audio_tokens)
                  : usage.audioTokens,
              videoTokens:
                parsedUsage.prompt_tokens_details?.video_tokens !== undefined
                  ? Number(parsedUsage.prompt_tokens_details.video_tokens)
                  : usage.videoTokens,
            };
          }

          // Handle content delta
          if (delta?.content) {
            totalContent += delta.content;
            yield { type: "delta", content: delta.content };
          }

          // Handle reasoning deltas (covers delta.reasoning, delta.reasoning_content, delta.reasoning_details)
          const reasoningChunk = delta?.reasoning ?? delta?.reasoning_content;
          if (reasoningChunk) {
            totalReasoning += String(reasoningChunk);
            yield { type: "delta", content: "", reasoning: String(reasoningChunk) };
          } else if (Array.isArray(delta?.reasoning_details)) {
            for (const detail of delta.reasoning_details) {
              const piece = typeof detail?.text === "string" ? detail.text : "";
              if (!piece) continue;
              totalReasoning += piece;
              yield { type: "delta", content: "", reasoning: piece };
            }
          }

          // Handle tool call deltas (streamed incrementally)
          if (delta?.tool_calls) {
            for (const tc of delta.tool_calls) {
              const idx = tc.index ?? 0;
              if (!pendingToolCalls.has(idx)) {
                pendingToolCalls.set(idx, { id: tc.id || "", name: "", args: "" });
              }
              const pending = pendingToolCalls.get(idx)!;
              if (tc.id) pending.id = tc.id;
              if (tc.function?.name) pending.name += tc.function.name;
              if (tc.function?.arguments) pending.args += tc.function.arguments;
            }
          }
        } catch {
          // skip malformed JSON chunks
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

  // Stream ended without [DONE] — yield accumulated tool calls or content
  if (pendingToolCalls.size > 0) {
    yield {
      type: "tool_calls",
      content: totalContent,
      reasoning: totalReasoning,
      toolCalls: Array.from(pendingToolCalls.values()).map((tc) => ({
        id: tc.id,
        type: "function" as const,
        function: { name: tc.name, arguments: tc.args },
      })),
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      totalTokens: usage.totalTokens,
      cost: usage.cost,
      reasoningTokens: usage.reasoningTokens,
      audioTokens: usage.audioTokens,
      videoTokens: usage.videoTokens,
    };
  } else {
    yield {
      type: "done",
      content: totalContent,
      reasoning: totalReasoning,
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      totalTokens: usage.totalTokens,
      cost: usage.cost,
      reasoningTokens: usage.reasoningTokens,
      audioTokens: usage.audioTokens,
      videoTokens: usage.videoTokens,
    };
  }
}

async function* parseResponsesSSE(
  body: ReadableStream<Uint8Array>,
  provider: AgentProviderId,
  signal?: AbortSignal,
): AsyncGenerator<ChatChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let totalContent = "";
  let totalReasoning = "";
  const pendingToolCalls = new Map<number, { id: string; name: string; args: string }>();
  let inputTokens = 0;
  let outputTokens = 0;
  let responseId: string | undefined;
  let sawAnyJson = false;
  let sawResponsesEvent = false;

  try {
    while (true) {
      if (signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (!line.startsWith("data: ")) continue;
        const data = line.slice(6).trim();
        if (!data || data === "[DONE]") continue;

        let parsed: any;
        try {
          parsed = JSON.parse(data);
        } catch {
          continue;
        }
        sawAnyJson = true;

        if (parsed?.choices) {
          throw new TransportCompatibilityError(
            "endpoint returned Chat Completions events for a Responses request",
          );
        }

        const eventType = typeof parsed?.type === "string" ? parsed.type : "";
        if (eventType.startsWith("response.") || eventType === "error") {
          sawResponsesEvent = true;
        }

        switch (eventType) {
          case "response.created":
            responseId = typeof parsed?.response?.id === "string" ? parsed.response.id : responseId;
            break;
          case "response.output_text.delta":
            if (typeof parsed?.delta === "string" && parsed.delta) {
              totalContent += parsed.delta;
              yield { type: "delta", content: parsed.delta };
            }
            break;
          case "response.reasoning_summary_text.delta":
            if (typeof parsed?.delta === "string" && parsed.delta) {
              totalReasoning += parsed.delta;
              yield { type: "delta", content: "", reasoning: parsed.delta };
            }
            break;
          case "response.output_item.added":
          case "response.output_item.done": {
            const outputIndex = Number(parsed?.output_index ?? 0);
            const item = parsed?.item;
            if (item?.type === "function_call") {
              const entry = pendingToolCalls.get(outputIndex) ?? { id: "", name: "", args: "" };
              if (typeof item?.call_id === "string") entry.id = item.call_id;
              if (typeof item?.name === "string") entry.name = item.name;
              if (typeof item?.arguments === "string") entry.args = item.arguments;
              pendingToolCalls.set(outputIndex, entry);
            }
            break;
          }
          case "response.function_call_arguments.delta": {
            const outputIndex = Number(parsed?.output_index ?? 0);
            const entry = pendingToolCalls.get(outputIndex) ?? { id: "", name: "", args: "" };
            if (typeof parsed?.delta === "string") entry.args += parsed.delta;
            pendingToolCalls.set(outputIndex, entry);
            break;
          }
          case "response.completed":
          case "response.incomplete":
            inputTokens = Number(parsed?.response?.usage?.input_tokens ?? inputTokens);
            outputTokens = Number(parsed?.response?.usage?.output_tokens ?? outputTokens);
            if (pendingToolCalls.size > 0) {
              yield {
                type: "tool_calls",
                content: totalContent,
                reasoning: totalReasoning || undefined,
                toolCalls: Array.from(pendingToolCalls.values()).map((toolCall) => ({
                  id: toolCall.id,
                  type: "function" as const,
                  function: { name: toolCall.name, arguments: toolCall.args },
                })),
                inputTokens,
                outputTokens,
                responseId,
              };
            } else {
              yield {
                type: "done",
                content: totalContent,
                reasoning: totalReasoning || undefined,
                inputTokens,
                outputTokens,
                responseId,
              };
            }
            return;
          case "error":
            yield {
              type: "error",
              content: typeof parsed?.message === "string" ? parsed.message : "Responses API error",
            };
            return;
          default:
            break;
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

  if (sawAnyJson && !sawResponsesEvent) {
    throw new TransportCompatibilityError(
      `${provider} did not return recognizable Responses API events`,
    );
  }

  if (pendingToolCalls.size > 0) {
    yield {
      type: "tool_calls",
      content: totalContent,
      reasoning: totalReasoning || undefined,
      toolCalls: Array.from(pendingToolCalls.values()).map((toolCall) => ({
        id: toolCall.id,
        type: "function" as const,
        function: { name: toolCall.name, arguments: toolCall.args },
      })),
      inputTokens,
      outputTokens,
      responseId,
    };
    return;
  }

  yield {
    type: "done",
    content: totalContent,
    reasoning: totalReasoning || undefined,
    inputTokens,
    outputTokens,
    responseId,
  };
}

async function* parseAnthropicSSE(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal,
): AsyncGenerator<ChatChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let totalContent = "";
  let totalReasoning = "";
  let inputTokens = 0;
  let outputTokens = 0;
  let inThinkingBlock = false;

  try {
    while (true) {
      if (signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (!line.startsWith("data: ")) continue;
        const data = line.slice(6).trim();

        try {
          const parsed = JSON.parse(data);

          if (parsed.type === "content_block_start") {
            const blockType = parsed.content_block?.type;
            inThinkingBlock = blockType === "thinking";
          } else if (parsed.type === "content_block_stop") {
            inThinkingBlock = false;
          } else if (parsed.type === "content_block_delta") {
            const deltaType = parsed.delta?.type;
            if (deltaType === "thinking_delta") {
              // Extended thinking delta
              const thinking = parsed.delta?.thinking ?? "";
              if (thinking) {
                totalReasoning += thinking;
                yield { type: "delta", content: "", reasoning: thinking };
              }
            } else if (deltaType === "text_delta") {
              if (inThinkingBlock) {
                // Thinking block text delivered as text_delta
                const text = parsed.delta?.text ?? "";
                if (text) {
                  totalReasoning += text;
                  yield { type: "delta", content: "", reasoning: text };
                }
              } else {
                const delta = parsed.delta?.text ?? "";
                if (delta) {
                  totalContent += delta;
                  yield { type: "delta", content: delta };
                }
              }
            }
          } else if (parsed.type === "message_start") {
            inputTokens = parsed.message?.usage?.input_tokens ?? 0;
          } else if (parsed.type === "message_delta") {
            outputTokens = parsed.usage?.output_tokens ?? 0;
          } else if (parsed.type === "message_stop") {
            yield {
              type: "done",
              content: totalContent,
              reasoning: totalReasoning || undefined,
              inputTokens,
              outputTokens,
            };
            return;
          }
        } catch {
          // skip malformed chunks
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

  yield {
    type: "done",
    content: totalContent,
    reasoning: totalReasoning || undefined,
    inputTokens,
    outputTokens,
  };
}

/**
 * Convert AgentMessage history to the format needed for API calls.
 */
export function messagesToApiFormat(
  messages: AgentMessage[],
): ApiChatMessage[] {
  const announcedToolCalls = new Set<string>();
  const emittedToolResults = new Set<string>();
  const apiMessages: ApiChatMessage[] = [];
  for (const m of messages) {
    if (m.isCompactionSummary) {
      continue;
    }
    if (!(m.role === "user" || m.role === "assistant" || m.role === "tool")) {
      continue;
    }
    if (m.role === "tool" && !(m.toolCallId && (m.toolStatus === "done" || m.toolStatus === "error"))) {
      continue;
    }

      if (m.role === "assistant" && Array.isArray(m.toolCalls)) {
        for (const toolCall of m.toolCalls) {
          if (toolCall.id?.trim()) {
            announcedToolCalls.add(toolCall.id);
          }
        }
      }

      if (m.role === "tool") {
        if (!m.toolCallId || !announcedToolCalls.has(m.toolCallId) || emittedToolResults.has(m.toolCallId)) {
          continue;
        }
        emittedToolResults.add(m.toolCallId);
        apiMessages.push({
          role: "tool",
          content: m.content,
          tool_call_id: m.toolCallId,
        } satisfies ApiChatMessage);
        continue;
      }

      apiMessages.push({
        role: m.role,
        content: m.content,
        tool_calls: m.role === "assistant" ? m.toolCalls : undefined,
      } satisfies ApiChatMessage);
  }
  return apiMessages;
}

export function buildApiMessagesForRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
): ApiChatMessage[] {
  return messagesToApiFormat(compactMessagesForRequest(messages, settings));
}

export function prepareOpenAIRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
  provider: AgentProviderId,
  model: string,
  requestedTransport: ApiTransportMode,
  authSource?: "api_key" | "chatgpt_subscription",
  assistantId?: string,
  thread?: Pick<AgentThread, "upstreamThreadId" | "upstreamTransport" | "upstreamProvider" | "upstreamModel" | "upstreamAssistantId">,
): PreparedOpenAIRequest {
  let selectedTransport = getSupportedApiTransports(provider).includes(requestedTransport)
    ? requestedTransport
    : getDefaultApiTransport(provider);
  if (provider === "openai" && authSource === "chatgpt_subscription") {
    selectedTransport = "responses";
  }
  const compacted = compactMessagesForRequest(messages, settings);
  const compactionActive =
    compacted.length !== messages.length || compacted.some((message) => message.content.startsWith("[Compacted earlier context]"));

  if (selectedTransport === "native_assistant" && assistantId?.trim()) {
    const latestUserMessage = [...messages]
      .reverse()
      .find((message) => message.role === "user" && !message.isCompactionSummary);
    if (latestUserMessage) {
      return {
        messages: messagesToApiFormat([latestUserMessage]),
        transport: "native_assistant",
        upstreamThreadId:
          thread?.upstreamTransport === "native_assistant"
            && thread.upstreamProvider === provider
            && thread.upstreamModel === model
            && thread.upstreamAssistantId === assistantId
            ? thread.upstreamThreadId ?? undefined
            : undefined,
      };
    }
  }

  if (selectedTransport === "responses") {
    if (!compactionActive && providerSupportsResponseContinuity(provider)) {
      const responseAnchorIndex = [...messages.keys()].reverse().find((index) => {
        const message = messages[index];
        return message.role === "assistant"
          && typeof message.responseId === "string"
          && message.provider === provider
          && message.model === model
          && message.apiTransport === "responses";
      });

      if (responseAnchorIndex !== undefined) {
        const trailingMessages = messagesToApiFormat(messages.slice(responseAnchorIndex + 1));
        if (trailingMessages.length > 0) {
          return {
            messages: trailingMessages,
            transport: "responses",
            previousResponseId: messages[responseAnchorIndex]?.responseId,
          };
        }
      }
    }

    return {
      messages: messagesToApiFormat(compacted),
      transport: "responses",
    };
  }

  return {
    messages: messagesToApiFormat(compacted),
    transport: "chat_completions",
  };
}

function compactMessagesForRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
): AgentMessage[] {
  if (messages.length === 0 || !settings.autoCompactContext) {
    return messages;
  }

  const targetTokens = effectiveContextTargetTokens(settings);
  const maxMessages = Math.max(1, Number(settings.maxContextMessages || 100));
  if (estimateMessageTokens(messages) <= targetTokens && messages.length <= maxMessages) {
    return messages;
  }

  const keepRecent = Math.min(
    Math.max(messages.length - 1, 0),
    Math.max(1, Number(settings.keepRecentOnCompaction || 10)),
  );
  const splitAt = Math.max(messages.length - keepRecent, 0);
  const olderMessages = messages.slice(0, splitAt);
  const recentMessages = messages.slice(splitAt);

  if (olderMessages.length === 0) {
    return trimCompactedMessages([...messages], targetTokens);
  }

  const summaryMessage: AgentMessage = {
    id: `compacted_${olderMessages[0]?.id ?? "history"}`,
    threadId: olderMessages[0]?.threadId ?? messages[0]?.threadId ?? "",
    createdAt: olderMessages[0]?.createdAt ?? messages[0]?.createdAt ?? Date.now(),
    role: "assistant",
    content: buildCompactionSummary(olderMessages, targetTokens),
    provider: undefined,
    model: undefined,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    reasoning: undefined,
    isCompactionSummary: false,
    isStreaming: false,
  };

  return trimCompactedMessages([summaryMessage, ...recentMessages], targetTokens);
}

function trimCompactedMessages(messages: AgentMessage[], targetTokens: number): AgentMessage[] {
  const trimmed = [...messages];
  while (trimmed.length > 1 && estimateMessageTokens(trimmed) > targetTokens) {
    trimmed.splice(0, 1);
  }
  return trimmed;
}

function effectiveContextTargetTokens(settings: ContextCompactionSettings): number {
  const contextWindow = Math.max(1, Number(settings.contextWindowTokens || 128000));
  const budgetTokens = Math.max(
    MIN_CONTEXT_TARGET_TOKENS,
    Number(settings.contextBudgetTokens || 100000),
  );
  const thresholdPercent = Math.min(
    100,
    Math.max(1, Number(settings.compactThresholdPercent || 80)),
  );
  return Math.max(
    MIN_CONTEXT_TARGET_TOKENS,
    Math.min(budgetTokens, Math.floor((contextWindow * thresholdPercent) / 100)),
  );
}

function estimateMessageTokens(messages: AgentMessage[]): number {
  return messages.reduce((sum, message) => sum + estimateSingleMessageTokens(message), 0);
}

function estimateSingleMessageTokens(message: AgentMessage): number {
  const text =
    message.content +
    (message.reasoning ?? "") +
    (message.toolArguments ?? "") +
    (message.toolName ?? "") +
    (message.toolCalls ? JSON.stringify(message.toolCalls) : "");
  return Math.ceil(text.length / APPROX_CHARS_PER_TOKEN) + 16;
}

function buildCompactionSummary(messages: AgentMessage[], targetTokens: number): string {
  const previewParts: string[] = [];

  for (const message of messages) {
    const summary = summarizeCompactedMessage(message);
    if (!summary) {
      continue;
    }
    previewParts.push(summary);
    const next = `[Compacted earlier context] Summary of older messages retained for continuity: ${previewParts.join(" | ")}`;
    if (Math.ceil(next.length / APPROX_CHARS_PER_TOKEN) >= targetTokens / 2) {
      break;
    }
  }

  const summaryBody =
    previewParts.length > 0
      ? previewParts.join(" | ")
      : `${messages.length} earlier messages were compacted.`;
  return `[Compacted earlier context] Summary of older messages retained for continuity: ${summaryBody}`;
}

function summarizeCompactedMessage(message: AgentMessage): string {
  let content = message.content.replace(/\s+/g, " ").trim();
  if (content.length > 160) {
    content = `${content.slice(0, 157)}...`;
  }

  if (message.role === "tool") {
    const toolName = message.toolName || "tool";
    return content ? `tool ${toolName}: ${content}` : `tool ${toolName} completed`;
  }

  const prefix = message.role === "assistant" ? "assistant" : message.role;
  return content ? `${prefix}: ${content}` : `${prefix}: (empty)`;
}
