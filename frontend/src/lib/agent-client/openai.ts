import type { AgentProviderId } from "../agentStore";
import { getProviderDefinition } from "../agentStore";
import type { ChatChunk, ChatRequest } from "./types";
import {
  applyDashScopeCodingPlanHeaders,
  applyOpenRouterAttributionHeaders,
  buildChatCompletionUrl,
  buildChatGptCodexHeaders,
  buildChatGptCodexResponsesUrl,
  buildNativeAssistantBaseUrl,
  buildResponsesUrl,
  isChatGptSubscriptionRequest,
  messageContentToText,
  TransportCompatibilityError,
  usesDashScopeEnableThinking,
} from "./shared";
import { parseResponsesSSE, parseSSEStream } from "./sse";

export async function* sendNativeAssistant(
  req: ChatRequest,
): AsyncGenerator<ChatChunk> {
  const providerDef = getProviderDefinition(req.provider);
  if (!providerDef?.nativeTransportKind) {
    throw new TransportCompatibilityError(
      `${req.provider} does not expose a native assistant API`,
    );
  }
  if (!req.config.assistant_id.trim()) {
    throw new TransportCompatibilityError(
      `${req.provider} native assistant requires Assistant ID`,
    );
  }

  const base_url = buildNativeAssistantBaseUrl(req.provider, req.config.base_url);
  const latestUserMessage = [...req.messages]
    .reverse()
    .find((message) => message.role === "user");
  const userText = latestUserMessage
    ? messageContentToText(latestUserMessage).trim()
    : "";
  if (!userText) {
    throw new Error("native assistant requires a user message");
  }

  const authHeaders: HeadersInit = {
    "Content-Type": "application/json",
    ...(req.config.api_key ? { Authorization: `Bearer ${req.config.api_key}` } : {}),
  };
  const compatibilityStatuses = new Set([400, 404, 405, 422]);

  let threadId = req.upstreamThreadId?.trim();
  if (!threadId) {
    const threadResponse = await fetch(`${base_url}/threads`, {
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
      yield {
        type: "error",
        content: `${req.provider} API returned ${threadResponse.status}: ${text.slice(0, 200)}`,
      };
      return;
    }
    const threadJson = await threadResponse.json();
    threadId = typeof threadJson?.id === "string" ? threadJson.id : "";
    if (!threadId) {
      yield {
        type: "error",
        content: `${req.provider} native assistant returned no thread id.`,
      };
      return;
    }
  }

  const messageResponse = await fetch(`${base_url}/threads/${threadId}/messages`, {
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
    yield {
      type: "error",
      content: `${req.provider} API returned ${messageResponse.status}: ${text.slice(0, 200)}`,
    };
    return;
  }

  const runResponse = await fetch(`${base_url}/threads/${threadId}/runs`, {
    method: "POST",
    headers: authHeaders,
    body: JSON.stringify({ assistant_id: req.config.assistant_id }),
    signal: req.signal,
  });
  if (!runResponse.ok) {
    const text = await runResponse.text().catch(() => "");
    if (compatibilityStatuses.has(runResponse.status)) {
      throw new TransportCompatibilityError(
        `Native assistant run creation failed (${runResponse.status}): ${text.slice(0, 240)}`,
      );
    }
    yield {
      type: "error",
      content: `${req.provider} API returned ${runResponse.status}: ${text.slice(0, 200)}`,
    };
    return;
  }
  const runJson = await runResponse.json();
  const runId = typeof runJson?.id === "string" ? runJson.id : "";
  if (!runId) {
    yield {
      type: "error",
      content: `${req.provider} native assistant returned no run id.`,
    };
    return;
  }

  let inputTokens = 0;
  let outputTokens = 0;
  for (let attempt = 0; attempt < 180; attempt += 1) {
    if (req.signal?.aborted) {
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 1000));
    const statusResponse = await fetch(`${base_url}/threads/${threadId}/runs/${runId}`, {
      method: "GET",
      headers: authHeaders,
      signal: req.signal,
    });
    if (!statusResponse.ok) {
      const text = await statusResponse.text().catch(() => "");
      yield {
        type: "error",
        content: `${req.provider} API returned ${statusResponse.status}: ${text.slice(0, 200)}`,
      };
      return;
    }
    const statusJson = await statusResponse.json();
    inputTokens = Number(
      statusJson?.usage?.prompt_tokens ?? statusJson?.usage?.input_tokens ?? inputTokens,
    );
    outputTokens = Number(
      statusJson?.usage?.completion_tokens ??
      statusJson?.usage?.output_tokens ??
      outputTokens,
    );
    switch (statusJson?.status) {
      case "queued":
      case "in_progress":
        continue;
      case "completed": {
        const messagesResponse = await fetch(
          `${base_url}/threads/${threadId}/messages?order=desc&limit=20`,
          {
            method: "GET",
            headers: authHeaders,
            signal: req.signal,
          },
        );
        if (!messagesResponse.ok) {
          const text = await messagesResponse.text().catch(() => "");
          yield {
            type: "error",
            content: `${req.provider} API returned ${messagesResponse.status}: ${text.slice(0, 200)}`,
          };
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
        yield {
          type: "error",
          content: `${req.provider} native assistant requested external tool action, which is not proxied in legacy mode.`,
        };
        return;
      case "failed":
      case "cancelled":
      case "expired":
        yield {
          type: "error",
          content:
            statusJson?.last_error?.message ||
            `${req.provider} native assistant run failed.`,
        };
        return;
      default:
        yield {
          type: "error",
          content: `${req.provider} native assistant entered unexpected status '${String(statusJson?.status ?? "")}'.`,
        };
        return;
    }
  }

  yield {
    type: "error",
    content: `${req.provider} native assistant timed out waiting for completion.`,
  };
}

export async function* sendOpenAICompatible(
  req: ChatRequest,
  chatgptAccountId?: string,
): AsyncGenerator<ChatChunk> {
  if (isChatGptSubscriptionRequest(req)) {
    yield* sendOpenAIResponses({
      ...req,
      config: {
        ...req.config,
        api_transport: "responses",
      },
      _chatgptAccountId: chatgptAccountId,
    });
    return;
  }

  const url = buildChatCompletionUrl(req.provider, req.config.base_url);
  const body: Record<string, unknown> = {
    model: req.config.model,
    messages: [{ role: "system", content: req.system_prompt }, ...req.messages],
    stream: req.streaming,
  };

  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools;
    body.tool_choice = "auto";
  }

  if (usesDashScopeEnableThinking(req.provider, req.config.model)) {
    body.enable_thinking = Boolean(
      req.reasoning_effort && req.reasoning_effort !== "none",
    );
  } else if (req.reasoning_effort && req.reasoning_effort !== "none") {
    body.reasoning_effort =
      req.reasoning_effort === "xhigh" ? "high" : req.reasoning_effort;
    body.reasoning = { effort: req.reasoning_effort };
  }

  if (req.streaming) {
    body.stream_options = { include_usage: true };
  }

  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (req.config.api_key) {
    headers.Authorization = `Bearer ${req.config.api_key}`;
  }
  applyOpenRouterAttributionHeaders(req.provider, headers);
  applyDashScopeCodingPlanHeaders(
    req.provider,
    req.config.base_url,
    "openai",
    headers,
  );

  const response = await fetch(url, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: req.signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    yield {
      type: "error",
      content: `${req.provider} API returned ${response.status}: ${text.slice(0, 200)}`,
    };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseSSEStream(response.body, req.signal);
  } else {
    const json = await response.json();
    const msg = json.choices?.[0]?.message;
    const content = msg?.content ?? "";

    if (msg?.tool_calls && msg.tool_calls.length > 0) {
      yield {
        type: "tool_calls",
        content,
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

export async function* sendOpenAIResponses(
  req: ChatRequest & { _chatgptAccountId?: string },
): AsyncGenerator<ChatChunk> {
  const isSubscription = isChatGptSubscriptionRequest(req);
  const url = isSubscription
    ? buildChatGptCodexResponsesUrl()
    : buildResponsesUrl(req.config.base_url);
  const body: Record<string, unknown> = {
    model: req.config.model,
    instructions: req.system_prompt,
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
  if (req.reasoning_effort && req.reasoning_effort !== "none") {
    body.reasoning = {
      effort: req.reasoning_effort === "xhigh" ? "high" : req.reasoning_effort,
    };
  }
  if (isSubscription) {
    body.include = ["reasoning.encrypted_content"];
    body.text = {
      ...(typeof body.text === "object" && body.text !== null
        ? (body.text as Record<string, unknown>)
        : {}),
      verbosity: "high",
    };
  }

  const headers = isSubscription
    ? buildChatGptCodexHeaders(req.config.api_key, req._chatgptAccountId)
    : (() => {
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
        ...(req.config.api_key
          ? { Authorization: `Bearer ${req.config.api_key}` }
          : {}),
      };
      applyOpenRouterAttributionHeaders(req.provider, headers);
      return headers;
    })();

  const response = await fetch(url, {
    method: "POST",
    headers,
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
    yield {
      type: "error",
      content: `${req.provider} API returned ${response.status}: ${text.slice(0, 200)}`,
    };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseResponsesSSE(response.body, req.provider as AgentProviderId, req.signal);
  } else {
    const json = await response.json();
    const responseId = typeof json?.id === "string" ? json.id : undefined;
    const output = Array.isArray(json?.output) ? json.output : [];
    const outputText = output
      .filter((item: any) => item?.type === "message")
      .flatMap((item: any) => (Array.isArray(item?.content) ? item.content : []))
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
