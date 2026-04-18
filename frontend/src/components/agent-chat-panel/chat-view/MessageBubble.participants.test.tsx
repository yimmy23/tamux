import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { MessageBubble } from "./MessageBubble";

describe("MessageBubble participant authorship", () => {
  it("renders the assistant author name when present", () => {
    const html = renderToStaticMarkup(
      <MessageBubble
        message={{
          id: "msg-1",
          threadId: "thread-1",
          createdAt: 1,
          role: "assistant",
          content: "I verified the claim.",
          authorAgentId: "weles",
          authorAgentName: "Weles",
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
          isStreaming: false,
        }}
      />,
    );

    expect(html).toContain("Weles");
    expect(html).toContain("I verified the claim.");
  });

  it("renders compaction trigger context for compaction artifacts", () => {
    const html = renderToStaticMarkup(
      <MessageBubble
        message={{
          id: "msg-compaction-1",
          threadId: "thread-1",
          createdAt: 1,
          role: "assistant",
          content:
            "Pre-compaction context: ~182,400 / 200,000 tokens (threshold 160,000)\nTrigger: message-count\nStrategy: rule based\n\nContent:\n# Compact summary\n- preserved goals",
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: true,
          messageKind: "compaction_artifact",
          isStreaming: false,
        }}
      />,
    );

    expect(html).toContain("auto compaction");
    expect(html).toContain("Pre-compaction context: ~182,400 / 200,000 tokens");
    expect(html).toContain("Trigger: message-count");
    expect(html).toContain("Strategy: rule based");
    expect(html).toContain("Content:");
    expect(html).toContain("Compact summary");
  });

  it("renders compaction payload when the artifact stores it separately from the header", () => {
    const html = renderToStaticMarkup(
      <MessageBubble
        message={{
          id: "msg-compaction-2",
          threadId: "thread-1",
          createdAt: 1,
          role: "assistant",
          content:
            "Pre-compaction context: ~542,139 / 400,000 tokens (threshold 320,000)\nTrigger: token-threshold\nStrategy: custom model generated",
          compactionPayload:
            "# 🤖 Agent Context: State Checkpoint\n\n## 🎯 Primary Objective\n> Preserve the coding task and next step.",
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: true,
          messageKind: "compaction_artifact",
          isStreaming: false,
        }}
      />,
    );

    expect(html).toContain("Strategy: custom model generated");
    expect(html).toContain("Agent Context: State Checkpoint");
    expect(html).toContain("Preserve the coding task and next step.");
  });
});
