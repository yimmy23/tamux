import { describe, expect, it } from "vitest";
import {
  buildHydratedRemoteThread,
  isGatewayAgentThread,
  isInternalAgentThread,
} from "./history";

describe("agent thread classification", () => {
  it("recognizes internal daemon threads by id or title", () => {
    expect(isInternalAgentThread({ daemonThreadId: "dm:svarog:weles", title: "Review" })).toBe(true);
    expect(isInternalAgentThread({ title: "Internal DM · Swarog ↔ WELES" })).toBe(true);
    expect(isInternalAgentThread({ daemonThreadId: "thread-user-1", title: "Regular work" })).toBe(false);
  });

  it("recognizes gateway threads by daemon title", () => {
    expect(isGatewayAgentThread({ title: "slack Alice" })).toBe(true);
    expect(isGatewayAgentThread({ title: "discord Bob" })).toBe(true);
    expect(isGatewayAgentThread({ title: "Regular Conversation", lastMessagePreview: "[slack — Alice]: hello" })).toBe(true);
    expect(isGatewayAgentThread({ title: "Regular Conversation", lastMessagePreview: "plain message" })).toBe(false);
  });
});

describe("buildHydratedRemoteThread", () => {
  it("keeps internal daemon threads visible for the React thread browser", () => {
    const hydrated = buildHydratedRemoteThread(
      {
        id: "dm:svarog:weles",
        title: "Internal DM · Swarog ↔ WELES",
        messages: [
          {
            role: "assistant",
            content: "visible in internal tab",
            timestamp: 1,
          },
        ],
      },
      "Svarog",
    );

    expect(hydrated?.thread.daemonThreadId).toBe("dm:svarog:weles");
    expect(hydrated?.thread.title).toBe("Internal DM · Swarog ↔ WELES");
  });

  it("hydrates daemon runtime profile and active context-window token metadata", () => {
    const hydrated = buildHydratedRemoteThread(
      {
        id: "thread-runtime-context",
        title: "Runtime Context",
        profile_provider: "alibaba-coding-plan",
        profile_model: "glm-5",
        profile_reasoning_effort: "high",
        profile_context_window_tokens: 202_752,
        active_context_window_start: 2,
        active_context_window_end: 6,
        active_context_window_tokens: 12_345,
        messages: [],
      },
      "Svarog",
    );

    expect(hydrated?.thread.profileProvider).toBe("alibaba-coding-plan");
    expect(hydrated?.thread.profileModel).toBe("glm-5");
    expect(hydrated?.thread.profileReasoningEffort).toBe("high");
    expect(hydrated?.thread.profileContextWindowTokens).toBe(202_752);
    expect(hydrated?.thread.activeContextWindowStart).toBe(2);
    expect(hydrated?.thread.activeContextWindowEnd).toBe(6);
    expect(hydrated?.thread.activeContextWindowTokens).toBe(12_345);
  });
});
