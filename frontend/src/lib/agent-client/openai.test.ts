import { afterEach, describe, expect, it, vi } from "vitest";
import { sendOpenAICompatible, sendOpenAIResponses } from "./openai.ts";

describe("sendOpenAIResponses", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("omits previous_response_id and sends session continuity headers for ChatGPT subscription", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) =>
      new Response(
        JSON.stringify({
          id: "resp_1",
          output: [],
          usage: { input_tokens: 1, output_tokens: 1 },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        },
      ));
    vi.stubGlobal("fetch", fetchMock);

    const iterator = sendOpenAIResponses({
      provider: "openai",
      config: {
        base_url: "https://api.openai.com/v1",
        model: "gpt-5.4",
        custom_model_name: "",
        api_key: "token",
        assistant_id: "",
        api_transport: "responses",
        auth_source: "chatgpt_subscription",
        context_window_tokens: 128_000,
      },
      system_prompt: "system",
      messages: [{ role: "user", content: "hello" }],
      streaming: false,
      previousResponseId: "resp_123",
      upstreamThreadId: "thread-1",
      _chatgptAccountId: "acct-1",
    });

    await iterator.next();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0] as [RequestInfo | URL, RequestInit];
    const body = JSON.parse(String(init.body));
    const headers = init.headers as Record<string, string>;

    expect(body.previous_response_id).toBeUndefined();
    expect(headers["session_id"]).toBe("thread-1");
    expect(headers["x-client-request-id"]).toBe("thread-1");
    expect(headers["chatgpt-account-id"]).toBe("acct-1");
  });

  it("adds OpenRouter response cache header only when enabled", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      new Response(
        JSON.stringify({
          choices: [{ message: { content: "ok" } }],
          usage: { prompt_tokens: 1, completion_tokens: 1 },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        },
      ));
    vi.stubGlobal("fetch", fetchMock);

    const baseRequest = {
      provider: "openrouter" as const,
      config: {
        base_url: "https://openrouter.ai/api/v1",
        model: "anthropic/claude-sonnet-4.5",
        custom_model_name: "",
        api_key: "token",
        assistant_id: "",
        api_transport: "chat_completions" as const,
        auth_source: "api_key" as const,
        context_window_tokens: 128_000,
      },
      system_prompt: "system",
      messages: [{ role: "user", content: "hello" }],
      streaming: false,
    };

    await sendOpenAICompatible({
      ...baseRequest,
      config: {
        ...baseRequest.config,
        openrouter_response_cache_enabled: false,
      },
    }).next();
    await sendOpenAICompatible({
      ...baseRequest,
      config: {
        ...baseRequest.config,
        openrouter_response_cache_enabled: true,
      },
    }).next();

    const firstHeaders = fetchMock.mock.calls[0][1]?.headers as Record<string, string>;
    const secondHeaders = fetchMock.mock.calls[1][1]?.headers as Record<string, string>;

    expect(firstHeaders["X-OpenRouter-Cache"]).toBeUndefined();
    expect(secondHeaders["X-OpenRouter-Cache"]).toBe("true");
  });
});
