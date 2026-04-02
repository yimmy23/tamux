import type { ApiChatMessage, ChatChunk, ChatRequest } from "./types";
import {
  applyDashScopeCodingPlanHeaders,
  isDashScopeCodingPlanAnthropicBaseUrl,
} from "./shared";
import { parseAnthropicSSE } from "./sse";

export async function* sendAnthropic(
  req: ChatRequest,
): AsyncGenerator<ChatChunk> {
  const url = `${req.config.base_url.replace(/\/$/, "")}/v1/messages`;

  const anthropicMessages = sanitizeAnthropicMessages(req.messages).map((m) => {
    const role =
      m.role === "system" ? "user" : m.role === "tool" ? "user" : m.role;

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
    system: req.system_prompt,
    messages: anthropicMessages,
    stream: req.streaming,
  };

  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools.map((t) => ({
      name: t.function.name,
      description: t.function.description,
      input_schema: t.function.parameters,
    }));
  }

  if (req.reasoning_effort && req.reasoning_effort !== "none") {
    const budgetMap: Record<string, number> = {
      minimal: 512,
      low: 1024,
      medium: 4096,
      high: 8192,
      xhigh: 16384,
    };
    const budgetTokens = budgetMap[req.reasoning_effort] ?? 4096;
    body.thinking = { type: "enabled", budget_tokens: budgetTokens };
    body.max_tokens = Math.max(4096, budgetTokens + 4096);
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    "x-api-key": req.config.api_key,
    "anthropic-dangerous-direct-browser-access": "true",
  };
  if (!isDashScopeCodingPlanAnthropicBaseUrl(req.config.base_url)) {
    headers["anthropic-version"] = "2023-06-01";
  }
  applyDashScopeCodingPlanHeaders(
    req.provider,
    req.config.base_url,
    "anthropic",
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
      content: `Anthropic API returned ${response.status}: ${text.slice(0, 200)}`,
    };
    return;
  }

  if (req.streaming && response.body) {
    yield* parseAnthropicSSE(response.body, req.signal);
  } else {
    const json = await response.json();
    const toolUseBlocks = json.content?.filter((b: any) => b.type === "tool_use") ?? [];
    if (toolUseBlocks.length > 0) {
      const textContent =
        json.content
          ?.filter((b: any) => b.type === "text")
          .map((b: any) => b.text)
          .join("") ?? "";
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
      yield {
        type: "done",
        content: json.content?.[0]?.text ?? "",
        inputTokens: json.usage?.input_tokens ?? 0,
        outputTokens: json.usage?.output_tokens ?? 0,
      };
    }
  }
}

function sanitizeAnthropicMessages(messages: ApiChatMessage[]): ApiChatMessage[] {
  const out: ApiChatMessage[] = [];

  for (let index = 0; index < messages.length;) {
    const message = messages[index];

    if (message.role === "assistant" && Array.isArray(message.tool_calls) && message.tool_calls.length > 0) {
      const expectedIds = new Set(
        message.tool_calls
          .map((toolCall) => toolCall.id)
          .filter((id): id is string => Boolean(id)),
      );
      const results: ApiChatMessage[] = [];
      const matchedIds = new Set<string>();
      let nextIndex = index + 1;

      while (nextIndex < messages.length && messages[nextIndex].role === "tool") {
        const toolMessage = messages[nextIndex];
        if (toolMessage.tool_call_id && expectedIds.has(toolMessage.tool_call_id)) {
          results.push(toolMessage);
          matchedIds.add(toolMessage.tool_call_id);
        }
        nextIndex += 1;
      }

      const hasCompleteBatch = matchedIds.size === expectedIds.size;
      const sawNoFollowupMessages = nextIndex === index + 1;
      const isUnansweredLatestToolTurn =
        sawNoFollowupMessages && nextIndex === messages.length;

      if (hasCompleteBatch) {
        out.push(message, ...results);
      } else if (isUnansweredLatestToolTurn) {
        out.push(message);
      }

      index = nextIndex;
      continue;
    }

    if (message.role === "tool") {
      index += 1;
      continue;
    }

    out.push(message);
    index += 1;
  }

  return out;
}
