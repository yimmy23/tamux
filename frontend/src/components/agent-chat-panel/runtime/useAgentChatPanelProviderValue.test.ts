import { describe, expect, it, vi } from "vitest";
import type { AgentRun } from "@/lib/agentRuns";
import type { AgentThread } from "@/lib/agentStore";
import type { Workspace } from "@/lib/types";
import {
  deriveSpawnedAgentNavigationState,
  openSpawnedAgentThreadFromRun,
} from "./useAgentChatPanelProviderValue";

function makeRun(overrides: Partial<AgentRun> = {}): AgentRun {
  return {
    id: "run-root",
    task_id: "task-root",
    kind: "subagent",
    classification: "coding",
    title: "Root Agent",
    description: "Inspect the spawned agent",
    status: "in_progress",
    priority: "normal",
    progress: 50,
    created_at: 1,
    source: "daemon",
    runtime: "claude-code",
    ...overrides,
  };
}

function makeThread(id: string, title: string, daemonThreadId: string | null): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "tamux",
    title,
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

function makeWorkspace(sessionId: string): Workspace {
  return {
    id: "workspace-1",
    name: "Workspace",
    icon: "terminal",
    accentColor: "#000",
    cwd: "/repo",
    gitBranch: null,
    gitDirty: false,
    listeningPorts: [],
    unreadCount: 0,
    activeSurfaceId: "surface-1",
    createdAt: 1,
    surfaces: [
      {
        id: "surface-1",
        workspaceId: "workspace-1",
        name: "Surface",
        icon: "terminal",
        layoutMode: "bsp",
        layout: {
          type: "leaf",
          id: "pane-1",
          sessionId,
        },
        paneNames: {},
        paneIcons: {},
        activePaneId: "pane-1",
        canvasState: {
          panX: 0,
          panY: 0,
          zoomLevel: 1,
          previousView: null,
        },
        canvasPanels: [
          {
            id: "panel-1",
            paneId: "pane-1",
            panelType: "terminal",
            title: "Terminal",
            icon: "terminal",
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            status: "running",
            sessionId,
            url: null,
            cwd: "/repo",
            userRenamed: false,
            lastActivityAt: 1,
          },
        ],
        createdAt: 1,
      },
    ],
  };
}

describe("deriveSpawnedAgentNavigationState", () => {
  it("derives the visible tree and back-thread hint from active thread context", () => {
    const activeThread = makeThread("local-root", "Root Thread", "daemon-root");
    const previousThread = makeThread("local-parent", "Parent Thread", "daemon-parent");
    const state = deriveSpawnedAgentNavigationState({
      activeThread,
      threads: [activeThread, previousThread],
      threadHistoryStack: ["local-parent"],
      runs: [
        makeRun({
          id: "run-root",
          task_id: "task-root",
          thread_id: "daemon-root",
        }),
        makeRun({
          id: "run-child",
          task_id: "task-child",
          title: "Child Thread",
          thread_id: "daemon-child",
          parent_task_id: "task-root",
          parent_thread_id: "daemon-root",
          created_at: 2,
        }),
      ],
    });

    expect(state.tree?.anchor?.item.id).toBe("run-root");
    expect(state.tree?.anchor?.children[0]?.item.id).toBe("run-child");
    expect(state.canGoBackThread).toBe(true);
    expect(state.threadNavigationDepth).toBe(1);
    expect(state.backThreadTitle).toBe("Parent Thread");
  });
});

describe("openSpawnedAgentThreadFromRun", () => {
  it("hydrates a missing daemon thread into local state before navigating", async () => {
    const addMessage = vi.fn();
    const createThread = vi.fn(() => "local-child");
    const getRemoteThread = vi.fn(async () => ({
      id: "daemon-child",
      title: "Hydrated Child",
      messages: [
        {
          id: "message-1",
          role: "assistant",
          content: "ready",
          timestamp: 10,
          input_tokens: 0,
          output_tokens: 0,
        },
      ],
    }));
    const fetchThreadTodos = vi.fn(async () => [{ id: "todo-1" }]);
    const openSpawnedThread = vi.fn();
    const setThreadDaemonId = vi.fn();
    const setThreadTodos = vi.fn();

    const result = await openSpawnedAgentThreadFromRun({
      activeThreadId: "local-root",
      threads: [makeThread("local-root", "Root Thread", "daemon-root")],
      workspaces: [makeWorkspace("session-child")],
      run: makeRun({
        id: "run-child",
        task_id: "task-child",
        title: "Spawned Child",
        thread_id: "daemon-child",
        session_id: "session-child",
        parent_task_id: "task-root",
        parent_thread_id: "daemon-root",
      }),
      messageLimit: 75,
      getRemoteThread,
      fetchThreadTodos,
      createThread,
      addMessage,
      setThreadDaemonId,
      setThreadTodos,
      openSpawnedThread,
    });

    expect(result).toBe(true);
    expect(getRemoteThread).toHaveBeenCalledWith("daemon-child", { messageLimit: 75 });
    expect(createThread).toHaveBeenCalledWith({
      workspaceId: "workspace-1",
      surfaceId: "surface-1",
      paneId: "pane-1",
      title: "Hydrated Child",
    });
    expect(setThreadDaemonId).toHaveBeenCalledWith("local-child", "daemon-child");
    expect(addMessage).toHaveBeenCalledTimes(1);
    expect(setThreadTodos).toHaveBeenCalledWith("local-child", [{ id: "todo-1" }]);
    expect(openSpawnedThread).toHaveBeenCalledWith("local-root", "local-child");
  });
});
