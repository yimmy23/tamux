import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";
import {
  loadDaemonThreadPageIntoLocalState,
  refreshDaemonThreadMetadataIntoLocalState,
  trimDaemonThreadMessagesToLatestWindow,
} from "./daemonHelpers";

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

function makeMessage(index: number, threadId = "local-active"): AgentMessage {
  return {
    id: `message-${index}`,
    threadId,
    role: index % 2 === 0 ? "user" : "assistant",
    content: `message ${index}`,
    createdAt: index,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
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

  it("prepends older daemon messages without trimming the expanded loaded range", async () => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 120,
          loadedMessageStart: 70,
          loadedMessageEnd: 120,
        },
      ],
      messages: {
        "local-active": Array.from({ length: 50 }, (_, index) => makeMessage(index + 70)),
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: Array.from({ length: 50 }, (_, index) => ({
        id: `message-${index + 20}`,
        role: (index + 20) % 2 === 0 ? "user" : "assistant",
        content: `message ${index + 20}`,
        timestamp: index + 20,
      })),
      total_message_count: 120,
      loaded_message_start: 20,
      loaded_message_end: 70,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 50,
      mergeMode: "prepend",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    const state = useAgentStore.getState();
    const thread = state.threads.find((entry) => entry.id === "local-active");
    expect(loaded).toBe(true);
    expect(state.messages["local-active"]).toHaveLength(100);
    expect(state.messages["local-active"]?.[0]?.id).toBe("message-20");
    expect(state.messages["local-active"]?.[99]?.id).toBe("message-119");
    expect(thread?.loadedMessageStart).toBe(20);
    expect(thread?.loadedMessageEnd).toBe(120);
  });
});

describe("trimDaemonThreadMessagesToLatestWindow", () => {
  beforeEach(() => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 120,
          loadedMessageStart: 20,
          loadedMessageEnd: 120,
        },
      ],
      messages: {
        "local-active": Array.from({ length: 100 }, (_, index) => makeMessage(index + 20)),
      },
      todos: {},
      activeThreadId: "local-active",
      threadHistoryStack: [],
    } as any);
  });

  it("keeps only the latest configured window and preserves loaded end", () => {
    const trimmed = trimDaemonThreadMessagesToLatestWindow({
      localThreadId: "local-active",
      messageLimit: 50,
    });

    const state = useAgentStore.getState();
    const thread = state.threads.find((entry) => entry.id === "local-active");
    expect(trimmed).toBe(true);
    expect(state.messages["local-active"]).toHaveLength(50);
    expect(state.messages["local-active"]?.[0]?.id).toBe("message-70");
    expect(state.messages["local-active"]?.[49]?.id).toBe("message-119");
    expect(thread?.loadedMessageStart).toBe(70);
    expect(thread?.loadedMessageEnd).toBe(120);
  });

  it("does nothing when the loaded messages already fit the configured window", () => {
    useAgentStore.setState({
      messages: {
        "local-active": Array.from({ length: 50 }, (_, index) => makeMessage(index + 70)),
      },
    } as any);

    const trimmed = trimDaemonThreadMessagesToLatestWindow({
      localThreadId: "local-active",
      messageLimit: 50,
    });

    expect(trimmed).toBe(false);
    expect(useAgentStore.getState().messages["local-active"]).toHaveLength(50);
  });
});
