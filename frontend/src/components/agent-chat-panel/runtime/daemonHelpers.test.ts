import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentThread } from "@/lib/agentStore";
import { loadDaemonThreadPageIntoLocalState, refreshDaemonThreadMetadataIntoLocalState } from "./daemonHelpers";

const agentGetThread = vi.fn();

vi.mock("@/lib/agentDaemonConfig", () => ({
  getAgentBridge: () => ({ agentGetThread }),
}));

vi.mock("@/lib/agentTodos", () => ({
  fetchThreadTodos: vi.fn(async () => []),
}));

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

describe("loadDaemonThreadPageIntoLocalState", () => {
  beforeEach(() => {
    agentGetThread.mockReset();
    useAgentStore.setState({
      threads: [
        makeThread("local-stale", "daemon-1"),
        makeThread("local-active", "daemon-1"),
      ],
      messages: {
        "local-stale": [],
        "local-active": [],
      },
      todos: {},
      activeThreadId: "local-active",
      threadHistoryStack: [],
    } as any);
  });

  it("loads daemon detail into the clicked local thread when duplicate daemon mappings exist", async () => {
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: [
        {
          id: "message-1",
          role: "user",
          content: "real daemon message",
          timestamp: 10,
        },
      ],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 75,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(loaded).toBe(true);
    expect(useAgentStore.getState().messages["local-active"]?.[0]?.content).toBe("real daemon message");
    expect(useAgentStore.getState().messages["local-stale"]).toEqual([]);
  });

  it("refreshes daemon thread metadata without replacing visible messages", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [{
          id: "local-message",
          threadId: "local-active",
          role: "assistant",
          content: "streaming local content",
          createdAt: 2,
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
        }],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Updated title",
      agent_name: "Svarog",
      messages: [
        {
          id: "remote-message",
          role: "user",
          content: "remote replacement should not land",
          timestamp: 10,
        },
      ],
      total_message_count: 27,
      loaded_message_start: 27,
      loaded_message_end: 27,
    });

    const refreshed = await refreshDaemonThreadMetadataIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(refreshed).toBe(true);
    expect(agentGetThread).toHaveBeenCalledWith("daemon-1", {
      messageLimit: 0,
      messageOffset: 0,
    });
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.title).toBe("Updated title");
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.messageCount).toBe(27);
    expect(useAgentStore.getState().messages["local-active"]?.[0]?.content).toBe("streaming local content");
  });
});
