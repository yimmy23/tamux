import { useEffect } from "react";
import { useAgentStore } from "@/lib/agentStore";
import { shouldUseDaemonRuntime, getAgentBridge } from "@/lib/agentDaemonConfig";
import { fetchAllThreadTodos, fetchThreadTodos } from "@/lib/agentTodos";
import { fetchGoalRuns, type GoalRun } from "@/lib/goalRuns";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentBackend } from "@/lib/agentStore/types";
import type { AgentThread, AgentTodoItem } from "@/lib/agentStore";
import type { WelesHealthState } from "@/lib/agentStore/types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import {
  appendDaemonSystemMessage,
  recordDaemonWorkflowNotice,
  reloadDaemonThreadIntoLocalState,
  syncWelesHealth,
} from "./daemonHelpers";
import {
  handleDivergentStartEvent,
  handleGatewayIncomingEvent,
  handleGoalRunEvent,
  handleOperatorProfileWarning,
  handlePayloadMessageEvent,
  handleTaskUpdateEvent,
  handleThreadCreatedEvent,
  handleTodoUpdateEvent,
  handleWorkspaceCommand,
} from "./daemonEventHandlers";

export function useDaemonAgentEvents({
  agentBackend,
  activePaneId,
  activeThread,
  activeThreadId,
  activeWorkspace,
  addMessage,
  createThread,
  setActiveThread,
  setThreadDaemonId,
  setThreadTodos,
  updateLastAssistantMessage,
  addNotification,
  daemonThreadIdRef,
  daemonLocalThreadRef,
  pendingGatewayMessagesRef,
  goalRunWorkspacesRef,
  setDaemonTodosByThread,
  setGoalRunsForTrace,
  setChatBackView,
  setLatestDivergentSessionId,
  setView,
  setWelesHealth,
}: {
  agentBackend: AgentBackend;
  activePaneId: string | null;
  activeThread: AgentThread | undefined;
  activeThreadId: string | null;
  activeWorkspace: ReturnType<ReturnType<typeof useWorkspaceStore.getState>["activeWorkspace"]>;
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  setActiveThread: ReturnType<typeof useAgentStore.getState>["setActiveThread"];
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  updateLastAssistantMessage: ReturnType<typeof useAgentStore.getState>["updateLastAssistantMessage"];
  addNotification: ReturnType<typeof import("@/lib/notificationStore").useNotificationStore.getState>["addNotification"];
  daemonThreadIdRef: MutableRefObject<string | null>;
  daemonLocalThreadRef: MutableRefObject<string | null>;
  pendingGatewayMessagesRef: MutableRefObject<Array<{ role: "user"; content: string; inputTokens: number; outputTokens: number; totalTokens: number; isCompactionSummary: boolean }>>;
  goalRunWorkspacesRef: MutableRefObject<Record<string, string>>;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
  setGoalRunsForTrace: Dispatch<SetStateAction<GoalRun[]>>;
  setChatBackView: Dispatch<SetStateAction<import("./types").AgentChatPanelView>>;
  setLatestDivergentSessionId: Dispatch<SetStateAction<string | null>>;
  setView: Dispatch<SetStateAction<import("./types").AgentChatPanelView>>;
  setWelesHealth: Dispatch<SetStateAction<WelesHealthState | null>>;
}) {
  useEffect(() => {
    daemonThreadIdRef.current = null;
    daemonLocalThreadRef.current = null;
    setDaemonTodosByThread({});
    setGoalRunsForTrace([]);
    setChatBackView("threads");
  }, [agentBackend, daemonLocalThreadRef, daemonThreadIdRef, setChatBackView, setDaemonTodosByThread, setGoalRunsForTrace]);

  useEffect(() => {
    if (!shouldUseDaemonRuntime(agentBackend)) return;
    void fetchAllThreadTodos().then(setDaemonTodosByThread);
    void fetchGoalRuns().then(setGoalRunsForTrace);
  }, [agentBackend, setDaemonTodosByThread, setGoalRunsForTrace]);

  useEffect(() => {
    if (!activeThread) {
      daemonThreadIdRef.current = null;
      daemonLocalThreadRef.current = null;
      return;
    }
    daemonLocalThreadRef.current = activeThread.id;
    daemonThreadIdRef.current = activeThread.daemonThreadId ?? null;
  }, [activeThread, daemonLocalThreadRef, daemonThreadIdRef]);

  useEffect(() => {
    const daemonThreadId = daemonThreadIdRef.current;
    const localThreadId = daemonLocalThreadRef.current;
    if (!daemonThreadId || !localThreadId || localThreadId !== activeThreadId) return;
    void fetchThreadTodos(daemonThreadId).then((items) => {
      setThreadTodos(localThreadId, items);
      setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: items }));
    });
  }, [activeThreadId, daemonLocalThreadRef, daemonThreadIdRef, setDaemonTodosByThread, setThreadTodos]);

  useEffect(() => {
    if (!shouldUseDaemonRuntime(agentBackend)) return;

    const amux = getAgentBridge();
    if (!amux?.onAgentEvent) return;

    const ensureStreamingAssistantMessage = (threadId: string) => {
      const messages = useAgentStore.getState().getThreadMessages(threadId);
      const last = messages[messages.length - 1];
      if (last?.role === "assistant" && last.isStreaming) {
        return last;
      }

      const isExternalAgent = agentBackend === "openclaw" || agentBackend === "hermes";
      const agentSettings = useAgentStore.getState().agentSettings;
      addMessage(threadId, {
        role: "assistant",
        content: "",
        provider: isExternalAgent ? agentBackend : agentSettings.active_provider,
        model: isExternalAgent
          ? agentBackend
          : ((agentSettings[agentSettings.active_provider] as any)?.model || ""),
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
        isStreaming: true,
      });

      const refreshedMessages = useAgentStore.getState().getThreadMessages(threadId);
      return refreshedMessages[refreshedMessages.length - 1];
    };

    const cleanupGoalRunWorkspace = (goalRunId: string) => {
      const workspaceId = goalRunWorkspacesRef.current[goalRunId];
      if (!workspaceId) return;
      setTimeout(() => {
        const currentStore = useWorkspaceStore.getState();
        if (currentStore.workspaces.some((workspace) => workspace.id === workspaceId)) {
          currentStore.closeWorkspace(workspaceId);
        }
        delete goalRunWorkspacesRef.current[goalRunId];
      }, 3000);
    };

    const unsubscribe = amux.onAgentEvent((event: any) => {
      if (!event?.type) return;

      const tid = daemonLocalThreadRef.current;

      switch (event.type) {
        case "delta": {
          if (!tid) break;
          const last = ensureStreamingAssistantMessage(tid);
          if (last?.role === "assistant" && last.isStreaming) {
            updateLastAssistantMessage(tid, (last.content || "") + (event.content || ""), true);
          }
          break;
        }
        case "reasoning": {
          if (!tid) break;
          const last = ensureStreamingAssistantMessage(tid);
          if (last?.role === "assistant" && last.isStreaming) {
            updateLastAssistantMessage(tid, last.content || "", true, {
              reasoning: (last.reasoning || "") + (event.content || ""),
            });
          }
          break;
        }
        case "done": {
          if (!tid) break;
          useAgentMissionStore.getState().setSharedCursorMode("idle");
          const last = ensureStreamingAssistantMessage(tid);
          if (last?.role === "assistant") {
            updateLastAssistantMessage(tid, last.content || "(empty)", false, {
              inputTokens: event.input_tokens ?? 0,
              outputTokens: event.output_tokens ?? 0,
              totalTokens: (event.input_tokens ?? 0) + (event.output_tokens ?? 0),
              provider: event.provider || undefined,
              model: event.model || undefined,
              tps: typeof event.tps === "number" ? event.tps : undefined,
              reasoning: event.reasoning || last.reasoning || undefined,
            });
          }
          break;
        }
        case "tool_call": {
          if (!tid) break;
          useAgentMissionStore.getState().setSharedCursorMode("agent");
          const messages = useAgentStore.getState().getThreadMessages(tid);
          const last = messages[messages.length - 1];
          if (last?.role === "assistant" && last.isStreaming) {
            updateLastAssistantMessage(tid, last.content || "Calling tools...", false);
          }
          addMessage(tid, {
            role: "tool",
            content: "",
            toolName: event.name,
            toolCallId: event.call_id,
            toolArguments: event.arguments,
            toolStatus: "requested",
            welesReview: event.weles_review || undefined,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          });
          break;
        }
        case "tool_result": {
          if (!tid) break;
          addMessage(tid, {
            role: "tool",
            content: event.content,
            toolName: event.name,
            toolCallId: event.call_id,
            toolStatus: event.is_error ? "error" : "done",
            welesReview: event.weles_review || undefined,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          });
          const isExternalAgent = agentBackend === "openclaw" || agentBackend === "hermes";
          const agentSettings = useAgentStore.getState().agentSettings;
          addMessage(tid, {
            role: "assistant",
            content: "",
            provider: isExternalAgent ? agentBackend : agentSettings.active_provider,
            model: isExternalAgent ? agentBackend : ((agentSettings[agentSettings.active_provider] as any)?.model || ""),
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
            isStreaming: true,
          });
          break;
        }
        case "error": {
          if (!tid) break;
          useAgentMissionStore.getState().setSharedCursorMode("idle");
          ensureStreamingAssistantMessage(tid);
          updateLastAssistantMessage(tid, `Error: ${event.message}`, false);
          break;
        }
        case "thread_created": {
          handleThreadCreatedEvent({
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
          });
          break;
        }
        case "thread_reload_required": {
          const reloadThreadId = typeof event.thread_id === "string" ? event.thread_id : null;
          if (reloadThreadId) {
            void reloadDaemonThreadIntoLocalState({
              daemonThreadId: reloadThreadId,
              setThreadTodos,
              setDaemonTodosByThread,
            });
          }
          break;
        }
        case "task_update":
          handleTaskUpdateEvent({ event, activePaneId, activeWorkspace, addNotification });
          break;
        case "goal_run_update":
        case "goal_run_created":
          handleGoalRunEvent({
            event,
            activePaneId,
            activeWorkspace,
            addNotification,
            cleanupGoalRunWorkspace,
            setGoalRunsForTrace,
          });
          break;
        case "todo_update":
          handleTodoUpdateEvent({
            event,
            daemonLocalThreadRef,
            daemonThreadIdRef,
            setDaemonTodosByThread,
            setThreadTodos,
          });
          break;
        case "workflow_notice":
          recordDaemonWorkflowNotice({ event, activePaneId, activeWorkspace });
          if (event.kind === "operator-profile-warning") {
            handleOperatorProfileWarning({ event, activePaneId, activeWorkspace, addNotification });
          }
          break;
        case "weles_health_update":
          syncWelesHealth(event, setWelesHealth, (content) => appendDaemonSystemMessage(content, daemonLocalThreadRef.current ?? activeThreadId));
          break;
        case "agent-divergent-session-started":
          handleDivergentStartEvent({ event, activeThreadId, daemonLocalThreadRef, setLatestDivergentSessionId });
          break;
        case "agent-divergent-session":
          handlePayloadMessageEvent(event, "Failed to fetch divergent session: ", "Divergent session payload", daemonLocalThreadRef.current ?? activeThreadId);
          break;
        case "agent-explanation":
          handlePayloadMessageEvent(event, "Failed to explain action: ", "Explainability", daemonLocalThreadRef.current ?? activeThreadId);
          break;
        case "workspace_command":
          handleWorkspaceCommand(event);
          break;
        case "concierge_welcome":
          useAgentStore.setState({
            conciergeWelcome: {
              content: event.content ?? "",
              actions: event.actions ?? [],
            },
          });
          break;
        case "gateway_incoming":
          handleGatewayIncomingEvent({ event, addMessage, daemonLocalThreadRef, pendingGatewayMessagesRef });
          break;
      }
    });

    return unsubscribe;
  }, [
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    addNotification,
    agentBackend,
    createThread,
    daemonLocalThreadRef,
    daemonThreadIdRef,
    goalRunWorkspacesRef,
    pendingGatewayMessagesRef,
    setActiveThread,
    setDaemonTodosByThread,
    setGoalRunsForTrace,
    setLatestDivergentSessionId,
    setThreadDaemonId,
    setThreadTodos,
    setView,
    setWelesHealth,
    updateLastAssistantMessage,
  ]);
}
