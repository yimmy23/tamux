import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig } from "./agentDaemonConfig.ts";
import {
  buildApiMessagesForRequest,
  resolveContextCompactionTargetTokens,
} from "./agent-client/context.ts";
import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentSettingsFromSource,
} from "./agentStore/settings.ts";

describe("agent compaction target", () => {
  const shortMessages = Array.from({ length: 101 }, (_, index) => ({
    id: `msg-${index}`,
    threadId: "thread-1",
    createdAt: index + 1,
    role: "user" as const,
    content: `m${index}`,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    isStreaming: false,
  }));

  it("does not serialize removed context budget settings", () => {
    expect(buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS)).not.toHaveProperty(
      "context_budget_tokens",
    );
  });

  it("drops legacy context budget values during frontend normalization", () => {
    const normalized = normalizeAgentSettingsFromSource({
      context_budget_tokens: 222_000,
    } as any);

    expect(normalized).not.toHaveProperty("context_budget_tokens");
  });

  it("uses the primary model threshold for heuristic compaction", () => {
    expect(
      resolveContextCompactionTargetTokens({
        auto_compact_context: true,
        max_context_messages: 100,
        context_window_tokens: 400_000,
        compact_threshold_pct: 80,
        keep_recent_on_compact: 10,
        compaction: DEFAULT_AGENT_SETTINGS.compaction,
      }),
    ).toBe(320_000);
  });

  it("caps the target by the WELES compaction window", () => {
    expect(
      resolveContextCompactionTargetTokens({
        auto_compact_context: true,
        max_context_messages: 100,
        context_window_tokens: 400_000,
        compact_threshold_pct: 80,
        keep_recent_on_compact: 10,
        compaction: {
          ...DEFAULT_AGENT_SETTINGS.compaction,
          strategy: "weles",
          weles: {
            provider: "minimax-coding-plan",
            model: "MiniMax-M2.7",
            reasoning_effort: "medium",
          },
        },
      }),
    ).toBe(164_000);
  });

  it("caps the target by the custom compaction model window", () => {
    expect(
      resolveContextCompactionTargetTokens({
        auto_compact_context: true,
        max_context_messages: 100,
        context_window_tokens: 400_000,
        compact_threshold_pct: 80,
        keep_recent_on_compact: 10,
        compaction: {
          ...DEFAULT_AGENT_SETTINGS.compaction,
          strategy: "custom_model",
          custom_model: {
            ...DEFAULT_AGENT_SETTINGS.compaction.custom_model,
            context_window_tokens: 160_000,
          },
        },
      }),
    ).toBe(128_000);
  });

  it("keeps heuristic message-count compaction active", () => {
    const prepared = buildApiMessagesForRequest(shortMessages, {
      auto_compact_context: true,
      max_context_messages: 100,
      context_window_tokens: 400_000,
      compact_threshold_pct: 80,
      keep_recent_on_compact: 10,
      compaction: {
        ...DEFAULT_AGENT_SETTINGS.compaction,
        strategy: "heuristic",
      },
    });

    expect(prepared[0]?.content).toContain("[Compacted earlier context]");
  });

  it("does not compact custom-model requests on message count alone", () => {
    const prepared = buildApiMessagesForRequest(shortMessages, {
      auto_compact_context: true,
      max_context_messages: 100,
      context_window_tokens: 400_000,
      compact_threshold_pct: 80,
      keep_recent_on_compact: 10,
      compaction: {
        ...DEFAULT_AGENT_SETTINGS.compaction,
        strategy: "custom_model",
        custom_model: {
          ...DEFAULT_AGENT_SETTINGS.compaction.custom_model,
          context_window_tokens: 1_000_000,
        },
      },
    });

    expect(prepared).toHaveLength(101);
    expect(prepared[0]?.content).toBe("m0");
  });

  it("does not compact weles requests on message count alone", () => {
    const prepared = buildApiMessagesForRequest(shortMessages, {
      auto_compact_context: true,
      max_context_messages: 100,
      context_window_tokens: 400_000,
      compact_threshold_pct: 80,
      keep_recent_on_compact: 10,
      compaction: {
        ...DEFAULT_AGENT_SETTINGS.compaction,
        strategy: "weles",
        weles: {
          provider: "alibaba-coding-plan",
          model: "qwen3.6-plus",
          reasoning_effort: "medium",
        },
      },
    });

    expect(prepared).toHaveLength(101);
    expect(prepared[0]?.content).toBe("m0");
  });

  it("injects pinned messages using Unicode scalar counts instead of UTF-16 length", () => {
    const prepared = buildApiMessagesForRequest(
      [
        {
          id: "pin-1",
          threadId: "thread-1",
          createdAt: 1,
          role: "user" as const,
          content: "😀",
          pinnedForCompaction: true,
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
          isStreaming: false,
        },
        {
          id: "msg-2",
          threadId: "thread-1",
          createdAt: 2,
          role: "user" as const,
          content: "latest",
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
          isStreaming: false,
        },
      ],
      {
        auto_compact_context: true,
        max_context_messages: 1,
        context_window_tokens: 1,
        compact_threshold_pct: 100,
        keep_recent_on_compact: 1,
        compaction: {
          ...DEFAULT_AGENT_SETTINGS.compaction,
          strategy: "heuristic",
        },
      },
    );

    expect(prepared).toHaveLength(3);
    expect(prepared[1]).toMatchObject({
      role: "user",
      content: "😀",
    });
  });
});
