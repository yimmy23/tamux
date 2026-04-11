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
});