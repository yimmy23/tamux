import { describe, expect, it } from "vitest";

import type { AgentMessage } from "../../../lib/agentStore";
import { buildDisplayItems } from "./helpers";

function message(overrides: Partial<AgentMessage>): AgentMessage {
  return {
    id: overrides.id ?? "msg",
    threadId: overrides.threadId ?? "thread",
    createdAt: overrides.createdAt ?? 1,
    role: overrides.role ?? "assistant",
    content: overrides.content ?? "",
    inputTokens: overrides.inputTokens ?? 0,
    outputTokens: overrides.outputTokens ?? 0,
    totalTokens: overrides.totalTokens ?? 0,
    isCompactionSummary: overrides.isCompactionSummary ?? false,
    ...overrides,
  };
}

describe("buildDisplayItems", () => {
  it("hides assistant tool placeholders around collapsed tool rows", () => {
    const items = buildDisplayItems([
      message({
        id: "user",
        role: "user",
        content: "Run ls",
        createdAt: 1,
      }),
      message({
        id: "assistant-tool-start",
        role: "assistant",
        content: "Calling tools...",
        createdAt: 2,
      }),
      message({
        id: "tool-requested",
        role: "tool",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolArguments: "{\"command\":\"ls\"}",
        toolStatus: "requested",
        createdAt: 3,
      }),
      message({
        id: "tool-done",
        role: "tool",
        content: "{\"status\":\"ok\"}",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolStatus: "done",
        createdAt: 4,
      }),
      message({
        id: "assistant-empty-after-tool",
        role: "assistant",
        content: "",
        createdAt: 5,
      }),
      message({
        id: "assistant-real-answer",
        role: "assistant",
        content: "The command completed.",
        createdAt: 6,
      }),
    ]);

    const rendered = items.map((item) => {
      if (item.type === "tool") {
        return `tool:${item.group.toolName}:${item.group.status}`;
      }
      return `message:${item.message.content || "<empty>"}`;
    });

    expect(rendered).toEqual([
      "message:Run ls",
      "tool:bash_command:done",
      "message:The command completed.",
    ]);
  });
});
