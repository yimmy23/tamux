import { beforeEach, describe, expect, it } from "vitest";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";
import {
  hasOpenLocalAssistantStream,
  isThreadlessDaemonStreamEvent,
  resolveDaemonEventLocalThreadId,
} from "./useDaemonAgentEvents";

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

function makeAssistantMessage(id: string, isStreaming: boolean): AgentMessage {
  return {
    id,
    threadId: "local-b",
    createdAt: 1,
    role: "assistant",
    content: "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    isStreaming,
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

  it("identifies stream events without thread ids as unsafe for active-thread fallback", () => {
    expect(isThreadlessDaemonStreamEvent({ type: "delta" })).toBe(true);
    expect(isThreadlessDaemonStreamEvent({ type: "reasoning" })).toBe(true);
    expect(isThreadlessDaemonStreamEvent({ type: "tool_call" })).toBe(true);
    expect(isThreadlessDaemonStreamEvent({ type: "workflow_notice" })).toBe(false);
    expect(isThreadlessDaemonStreamEvent({ type: "delta", thread_id: "daemon-b" })).toBe(false);
  });

  it("drops threadless stream events unless a local assistant stream is already open", () => {
    expect(resolveDaemonEventLocalThreadId(
      { type: "delta" },
      "local-b",
      "daemon-b",
      { allowThreadlessFallback: hasOpenLocalAssistantStream("local-b") },
    )).toBeNull();

    useAgentStore.setState({
      messages: {
        "local-b": [makeAssistantMessage("assistant-streaming", true)],
      },
    } as any);

    expect(resolveDaemonEventLocalThreadId(
      { type: "delta" },
      "local-b",
      "daemon-b",
      { allowThreadlessFallback: hasOpenLocalAssistantStream("local-b") },
    )).toBe("local-b");

    useAgentStore.setState({
      messages: {
        "local-b": [makeAssistantMessage("assistant-done", false)],
      },
    } as any);

    expect(resolveDaemonEventLocalThreadId(
      { type: "delta" },
      "local-b",
      "daemon-b",
      { allowThreadlessFallback: hasOpenLocalAssistantStream("local-b") },
    )).toBeNull();
  });
});
