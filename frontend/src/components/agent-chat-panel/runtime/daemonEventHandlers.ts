import { fetchGoalRuns, normalizeGoalRun } from "@/lib/goalRuns";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useSnippetStore } from "@/lib/snippetStore";
import { provisionTerminalPaneInWorkspace } from "@/lib/agentWorkspace";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { fetchThreadTodos } from "@/lib/agentTodos";
import type { AgentTodoItem } from "@/lib/agentStore";
import { TOOL_NAMES } from "@/lib/agentTools/toolNames";
import {
  appendDaemonSystemMessage,
  normalizeBridgePayload,
} from "./daemonHelpers";

export function handleThreadCreatedEvent({
  event,
  activePaneId,
  addMessage,
  createThread,
  daemonLocalThreadRef,
  daemonThreadIdRef,
  pendingGatewayMessagesRef,
  setActiveThread,
  setDaemonTodosByThread,
  setThreadDaemonId,
  setThreadTodos,
  setView,
}: any) {
  if (!event.thread_id) return;
  const isOurThread = !daemonLocalThreadRef.current
    || daemonThreadIdRef.current === null
    || event.thread_id === daemonThreadIdRef.current;
  if (isOurThread) {
    daemonThreadIdRef.current = event.thread_id;
  }
  if (daemonLocalThreadRef.current && isOurThread) {
    setThreadDaemonId(daemonLocalThreadRef.current, event.thread_id);
  }
  if (daemonLocalThreadRef.current) return;

  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
  const paneId = useWorkspaceStore.getState().activePaneId();
  const localId = createThread({
    workspaceId,
    surfaceId,
    paneId: activePaneId ?? paneId,
    title: event.title || "Gateway conversation",
  });
  daemonLocalThreadRef.current = localId;
  setThreadDaemonId(localId, event.thread_id);
  setActiveThread(localId);
  setView("chat");
  if (event.thread_id) {
    void fetchThreadTodos(event.thread_id).then((items) => {
      setThreadTodos(localId, items);
      setDaemonTodosByThread((current: Record<string, AgentTodoItem[]>) => ({ ...current, [event.thread_id]: items }));
    });
  }
  for (const buffered of pendingGatewayMessagesRef.current) {
    addMessage(localId, buffered);
  }
  pendingGatewayMessagesRef.current = [];
  addMessage(localId, {
    role: "assistant",
    content: "",
    provider: "daemon",
    model: "daemon",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    isStreaming: true,
  });
}

export function handleTaskUpdateEvent({ event, activePaneId, activeWorkspace, addNotification }: any) {
  const task = event.task;
  if (!task) return;
  if (task.status === "awaiting_approval") {
    const approvalId = task.awaiting_approval_id || task.id;
    useAgentMissionStore.getState().upsertDaemonApproval({
      id: approvalId,
      paneId: activePaneId ?? task.session_id ?? "",
      workspaceId: activeWorkspace?.id ?? null,
      surfaceId: null,
      sessionId: task.session_id ?? null,
      command: task.blocked_reason || task.title,
      reasons: [task.blocked_reason || "Managed command requires approval"],
      riskLevel: "medium",
      blastRadius: "task",
    });
    addNotification({
      title: "Task awaiting approval",
      body: task.title,
      subtitle: task.blocked_reason || "Managed command paused — check Trace tab",
      icon: "shield",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
  }
  if (task.status === "completed") {
    addNotification({
      title: task.retry_count > 0 ? "Task self-healed" : "Task completed",
      body: task.title,
      subtitle: task.retry_count > 0 ? `Recovered after ${task.retry_count} retry${task.retry_count === 1 ? "" : "ies"}` : "Background queue",
      icon: task.retry_count > 0 ? "sparkles" : "check",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
  }
  if (task.status === "failed") {
    addNotification({
      title: "Task failed",
      body: task.title,
      subtitle: task.last_error || event.message || "Retry budget exhausted",
      icon: "alert-triangle",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
  }
}

export function handleGoalRunEvent({ event, activePaneId, activeWorkspace, addNotification, cleanupGoalRunWorkspace, setGoalRunsForTrace }: any) {
  const goalRun = normalizeGoalRun(event.goal_run ?? event.goalRun ?? event.run ?? null);
  if (!goalRun) return;
  void fetchGoalRuns().then(setGoalRunsForTrace);
  if (goalRun.status === "awaiting_approval") {
    addNotification({
      title: "Goal runner awaiting approval",
      body: goalRun.title,
      subtitle: goalRun.current_step_title || "Managed command paused",
      icon: "shield",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
  }
  if (goalRun.status === "completed") {
    addNotification({
      title: "Goal runner completed",
      body: goalRun.title,
      subtitle: goalRun.generated_skill_path ? "Skill generated from successful run" : "Long-running autonomy",
      icon: "check",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
    cleanupGoalRunWorkspace(goalRun.id);
  }
  if (goalRun.status === "failed" || goalRun.status === "cancelled") {
    addNotification({
      title: goalRun.status === "cancelled" ? "Goal runner cancelled" : "Goal runner failed",
      body: goalRun.title,
      subtitle: goalRun.last_error || goalRun.error || goalRun.current_step_title || "Review the latest reflection",
      icon: "alert-triangle",
      source: "system",
      workspaceId: activeWorkspace?.id ?? null,
      paneId: activePaneId ?? null,
      panelId: activePaneId ?? null,
    });
    cleanupGoalRunWorkspace(goalRun.id);
  }
}

export function handleTodoUpdateEvent({ event, daemonLocalThreadRef, daemonThreadIdRef, setDaemonTodosByThread, setGoalRunsForTrace, setThreadTodos }: any) {
  const daemonThreadId = typeof event.thread_id === "string" ? event.thread_id : null;
  const localThreadId = daemonThreadId && daemonThreadIdRef.current === daemonThreadId
    ? daemonLocalThreadRef.current
    : null;
  if (!Array.isArray(event.items)) return;

  const todos = event.items
    .map((item: any, index: number): AgentTodoItem | null => {
      const content = typeof item?.content === "string" ? item.content.trim() : "";
      if (!content) return null;
      return {
        id: typeof item?.id === "string" ? item.id : `todo-${index}`,
        content,
        status: item?.status === "in_progress" || item?.status === "completed" || item?.status === "blocked"
          ? item.status
          : "pending",
        position: typeof item?.position === "number" ? item.position : index,
        stepIndex: typeof item?.step_index === "number"
          ? item.step_index
          : typeof item?.stepIndex === "number"
            ? item.stepIndex
            : null,
        createdAt: typeof item?.created_at === "number" ? item.created_at : typeof item?.createdAt === "number" ? item.createdAt : null,
        updatedAt: typeof item?.updated_at === "number" ? item.updated_at : typeof item?.updatedAt === "number" ? item.updatedAt : null,
      };
    })
    .filter((item: AgentTodoItem | null): item is AgentTodoItem => Boolean(item));

  if (daemonThreadId) {
    setDaemonTodosByThread((current: Record<string, AgentTodoItem[]>) => ({ ...current, [daemonThreadId]: todos }));
  }
  if (localThreadId) {
    setThreadTodos(localThreadId, todos);
  }
  if (typeof event.goal_run_id === "string" && event.goal_run_id.trim()) {
    void fetchGoalRuns().then(setGoalRunsForTrace);
  }
}

export function handleOperatorProfileWarning({ event, activePaneId, activeWorkspace, addNotification }: any) {
  const details = typeof event.details === "string" ? event.details : null;
  let retryAction: string | null = null;
  let warningBody = typeof event.message === "string" ? event.message : "Operator profile warning";
  if (details) {
    try {
      const parsed = JSON.parse(details);
      if (typeof parsed?.retry_action === "string") {
        retryAction = parsed.retry_action;
      }
      if (typeof parsed?.error === "string") {
        warningBody = `${warningBody}\n${parsed.error}`;
      }
    } catch {
      warningBody = `${warningBody}\n${details}`;
    }
  }
  addNotification({
    title: "Operator profile warning",
    body: warningBody,
    icon: "alert-triangle",
    source: "system",
    workspaceId: activeWorkspace?.id ?? null,
    paneId: activePaneId ?? null,
    panelId: activePaneId ?? null,
    actions: retryAction ? [{ id: retryAction, label: "Retry" }] : [],
  });
}

export function handleDivergentStartEvent({ event, activeThreadId, daemonLocalThreadRef, setLatestDivergentSessionId }: any) {
  const payload = normalizeBridgePayload(event);
  if (payload?.ok === false && typeof payload?.error === "string") {
    appendDaemonSystemMessage(`Failed to start divergent session: ${payload.error}`, daemonLocalThreadRef.current ?? activeThreadId);
    return;
  }
  const sessionId = typeof payload.session_id === "string" ? payload.session_id : null;
  if (sessionId) {
    setLatestDivergentSessionId(sessionId);
  }
  appendDaemonSystemMessage(
    sessionId
      ? `Divergent session started: \`${sessionId}\`.\nType \`!diverge-get\` to fetch it.`
      : "Divergent session started.",
    daemonLocalThreadRef.current ?? activeThreadId,
  );
}

export function handlePayloadMessageEvent(event: any, errorPrefix: string, title: string, threadId: string | null) {
  const payload = normalizeBridgePayload(event);
  if (payload?.ok === false && typeof payload?.error === "string") {
    appendDaemonSystemMessage(`${errorPrefix}${payload.error}`, threadId);
    return;
  }
  appendDaemonSystemMessage(
    `${title}\n\n\`\`\`json\n${JSON.stringify(payload, null, 2)}\n\`\`\``,
    threadId,
  );
}

export function handleWorkspaceCommand(event: any) {
  const command = event.command;
  const args = event.args || {};
  const store = useWorkspaceStore.getState();
  const snippetStore = useSnippetStore.getState();
  try {
    switch (command) {
      case TOOL_NAMES.createWorkspace:
        store.createWorkspace(args.name);
        break;
      case TOOL_NAMES.setActiveWorkspace: {
        const workspace = store.workspaces.find((entry: any) => entry.id === args.workspace || entry.name === args.workspace);
        if (workspace) store.setActiveWorkspace(workspace.id);
        break;
      }
      case TOOL_NAMES.createSurface:
        store.createSurface(undefined, undefined);
        break;
      case TOOL_NAMES.setActiveSurface: {
        const activeWorkspace = store.activeWorkspace();
        if (activeWorkspace) {
          const surface = activeWorkspace.surfaces.find((entry: any) => entry.id === args.surface || entry.name === args.surface);
          if (surface) store.setActiveSurface(surface.id);
        }
        break;
      }
      case TOOL_NAMES.splitPane:
        store.splitActive(args.direction === "vertical" ? "vertical" : "horizontal");
        break;
      case TOOL_NAMES.renamePane: {
        const paneId = args.pane || store.activePaneId();
        if (paneId) store.setPaneName(paneId, args.name);
        break;
      }
      case "attach_agent_terminal":
        if (typeof args.workspace_id === "string" && args.workspace_id) {
          void provisionTerminalPaneInWorkspace({
            workspaceId: args.workspace_id,
            paneName: typeof args.pane_name === "string" && args.pane_name ? args.pane_name : "Work",
            cwd: typeof args.cwd === "string" && args.cwd ? args.cwd : null,
            sessionId: typeof args.session_id === "string" && args.session_id ? args.session_id : null,
          });
        }
        break;
      case TOOL_NAMES.createSnippet:
        snippetStore.addSnippet({ name: args.name, content: args.content, category: args.category, description: args.description, tags: args.tags, owner: "assistant" });
        break;
    }
  } catch (error) {
    console.warn("workspace command failed:", command, error);
  }
}

export function handleGatewayIncomingEvent({ event, addMessage, daemonLocalThreadRef, pendingGatewayMessagesRef }: any) {
  const gatewayMessage = {
    role: "user" as const,
    content: `[${event.platform} — ${event.sender}]: ${event.content}`,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  };
  const threadId = daemonLocalThreadRef.current;
  if (threadId) {
    addMessage(threadId, gatewayMessage);
    addMessage(threadId, {
      role: "assistant",
      content: "",
      provider: "daemon",
      model: "daemon",
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
      isStreaming: true,
    });
    return;
  }
  pendingGatewayMessagesRef.current.push(gatewayMessage);
}
