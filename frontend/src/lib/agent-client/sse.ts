import type { AgentProviderId } from "../agentStore";
import type { ChatChunk } from "./types";
import { TransportCompatibilityError } from "./shared";

export async function* parseSSEStream(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal,
): AsyncGenerator<ChatChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let totalContent = "";
  let totalReasoning = "";
  const pendingToolCalls = new Map<number, { id: string; name: string; args: string }>();
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
              outputTokens: Number(
                parsedUsage.completion_tokens ?? usage.outputTokens ?? 0,
              ),
              totalTokens: Number(parsedUsage.total_tokens ?? usage.totalTokens ?? 0),
              cost:
                parsedUsage.cost !== undefined
                  ? Number(parsedUsage.cost)
                  : usage.cost,
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

          if (delta?.content) {
            totalContent += delta.content;
            yield { type: "delta", content: delta.content };
          }

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
          /* skip malformed JSON chunks */
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

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

export async function* parseResponsesSSE(
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
            responseId =
              typeof parsed?.response?.id === "string"
                ? parsed.response.id
                : responseId;
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
              const entry = pendingToolCalls.get(outputIndex) ?? {
                id: "",
                name: "",
                args: "",
              };
              if (typeof item?.call_id === "string") entry.id = item.call_id;
              if (typeof item?.name === "string") entry.name = item.name;
              if (typeof item?.arguments === "string") entry.args = item.arguments;
              pendingToolCalls.set(outputIndex, entry);
            }
            break;
          }
          case "response.function_call_arguments.delta": {
            const outputIndex = Number(parsed?.output_index ?? 0);
            const entry = pendingToolCalls.get(outputIndex) ?? {
              id: "",
              name: "",
              args: "",
            };
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
              content:
                typeof parsed?.message === "string"
                  ? parsed.message
                  : "Responses API error",
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

export async function* parseAnthropicSSE(
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
            inThinkingBlock = parsed.content_block?.type === "thinking";
          } else if (parsed.type === "content_block_stop") {
            inThinkingBlock = false;
          } else if (parsed.type === "content_block_delta") {
            const deltaType = parsed.delta?.type;
            if (deltaType === "thinking_delta") {
              const thinking = parsed.delta?.thinking ?? "";
              if (thinking) {
                totalReasoning += thinking;
                yield { type: "delta", content: "", reasoning: thinking };
              }
            } else if (deltaType === "text_delta") {
              if (inThinkingBlock) {
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
          /* skip malformed chunks */
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
