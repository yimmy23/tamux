import { beforeEach, describe, expect, it } from "vitest";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentThread } from "@/lib/agentStore";
import { resolveDaemonEventLocalThreadId } from "./useDaemonAgentEvents";

function makeThread(id: string, daemonThreadId: string): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "zorai",
    title: id,
    createdAt: 1,
    updatedAt: 1,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamThreadId: null,
    upstreamTransport: undefined,
    upstreamProvider: null,
    upstreamModel: null,
    upstreamAssistantId: null,
  };
}

describe("resolveDaemonEventLocalThreadId", () => {
  beforeEach(() => {
    useAgentStore.setState({
      threads: [
        makeThread("local-a", "daemon-a"),
        makeThread("local-b", "daemon-b"),
      ],
      messages: {},
      todos: {},
      activeThreadId: "local-b",
      threadHistoryStack: [],
    } as any);
  });

  it("routes stream events by daemon thread id instead of active fallback", () => {
    expect(resolveDaemonEventLocalThreadId(
      { type: "delta", thread_id: "daemon-a" },
      "local-b",
      "daemon-b",
    )).toBe("local-a");
  });

  it("drops foreign daemon events when their thread is not loaded locally", () => {
    expect(resolveDaemonEventLocalThreadId(
      { type: "delta", thread_id: "daemon-unknown" },
      "local-b",
      "daemon-b",
    )).toBeNull();
  });
});
