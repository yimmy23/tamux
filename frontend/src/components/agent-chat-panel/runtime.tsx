import React, { createContext, useContext, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { abortThreadStream, clearThreadAbortController, getEffectiveContextWindow, setThreadAbortController, useAgentStore } from "../../lib/agentStore";
import type { AgentMessage, AgentThread, AgentTodoItem, AgentProviderConfig } from "../../lib/agentStore";
import { prepareOpenAIRequest, sendChatCompletion } from "../../lib/agentClient";
import type { AgentSettings } from "../../lib/agentStore";
import { provisionAgentWorkspaceTerminals, provisionTerminalPaneInWorkspace, resolvePaneSessionId } from "../../lib/agentWorkspace";
import { buildHonchoContext, syncMessagesToHoncho } from "../../lib/honchoClient";
import { getAvailableTools, executeTool, getToolCapabilityDescription } from "../../lib/agentTools";
import { useAgentMissionStore } from "../../lib/agentMissionStore";
import { useNotificationStore } from "../../lib/notificationStore";
import { useSnippetStore } from "../../lib/snippetStore";
import { getTerminalController } from "../../lib/terminalRegistry";
import { useTranscriptStore } from "../../lib/transcriptStore";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import { fetchAllThreadTodos, fetchThreadTodos } from "../../lib/agentTodos";
import { fetchGoalRuns, goalRunSupportAvailable, normalizeGoalRun, startGoalRun, type GoalRun } from "../../lib/goalRuns";
import {
    buildDaemonAgentConfig,
    diffDaemonConfigEntries,
    getAgentBridge,
    shouldUseDaemonRuntime,
} from "../../lib/agentDaemonConfig";
import { AgentExecutionGraph } from "../AgentExecutionGraph";
import { AITrainingView } from "./AITrainingView";
import { ChatView } from "./ChatView";
import { CodingAgentsView } from "./CodingAgentsView";
import { ContextView } from "./ContextView";
import { SubagentsView } from "./SubagentsView";
import { TasksView } from "./TasksView";
import { MetricRibbon, SectionTitle, iconButtonStyle } from "./shared";
import { ThreadList } from "./ThreadList";
import { TraceView } from "./TraceView";
import { UsageView } from "./UsageView";

const EMPTY_MESSAGES: AgentMessage[] = [];

export type AgentChatPanelView = "threads" | "chat" | "trace" | "usage" | "context" | "graph" | "coding-agents" | "ai-training" | "tasks" | "subagents";

type AgentStoreState = ReturnType<typeof useAgentStore.getState>;
type AgentMissionStoreState = ReturnType<typeof useAgentMissionStore.getState>;
type WorkspaceStoreState = ReturnType<typeof useWorkspaceStore.getState>;
type SnippetStoreState = ReturnType<typeof useSnippetStore.getState>;
type TranscriptStoreState = ReturnType<typeof useTranscriptStore.getState>;

type AgentChatPanelRuntimeValue = {
    togglePanel: () => void;
    activeWorkspace: ReturnType<WorkspaceStoreState["activeWorkspace"]>;
    threads: AgentThread[];
    activeThread: AgentThread | undefined;
    activeThreadId: string | null;
    createThread: AgentStoreState["createThread"];
    deleteThread: AgentStoreState["deleteThread"];
    setActiveThread: AgentStoreState["setActiveThread"];
    agentSettings: AgentStoreState["agentSettings"];
    updateAgentSetting: AgentStoreState["updateAgentSetting"];
    searchQuery: string;
    setSearchQuery: (query: string) => void;
    messages: AgentMessage[];
    todos: AgentTodoItem[];
    daemonTodosByThread: Record<string, AgentTodoItem[]>;
    goalRunsForTrace: GoalRun[];
    allMessagesByThread: Record<string, AgentMessage[]>;
    pendingApprovals: AgentMissionStoreState["approvals"];
    scopedOperationalEvents: AgentMissionStoreState["operationalEvents"];
    scopedCognitiveEvents: AgentMissionStoreState["cognitiveEvents"];
    latestContextSnapshot: AgentMissionStoreState["contextSnapshots"][number] | undefined;
    memory: AgentMissionStoreState["memory"];
    updateMemory: AgentMissionStoreState["updateMemory"];
    historySummary: AgentMissionStoreState["historySummary"];
    historyHits: AgentMissionStoreState["historyHits"];
    symbolHits: AgentMissionStoreState["symbolHits"];
    snippets: SnippetStoreState["snippets"];
    transcripts: TranscriptStoreState["transcripts"];
    scopePaneId: string | null;
    scopeController: ReturnType<typeof getTerminalController>;
    input: string;
    setInput: React.Dispatch<React.SetStateAction<string>>;
    historyQuery: string;
    setHistoryQuery: React.Dispatch<React.SetStateAction<string>>;
    symbolQuery: string;
    setSymbolQuery: React.Dispatch<React.SetStateAction<string>>;
    view: AgentChatPanelView;
    setView: React.Dispatch<React.SetStateAction<AgentChatPanelView>>;
    chatBackView: AgentChatPanelView;
    setChatBackView: React.Dispatch<React.SetStateAction<AgentChatPanelView>>;
    usageMessageCount: number;
    filteredThreads: AgentThread[];
    isStreamingResponse: boolean;
    messagesEndRef: React.RefObject<HTMLDivElement | null>;
    inputRef: React.RefObject<HTMLTextAreaElement | null>;
    sendMessage: (text: string) => void;
    deleteMessage: (threadId: string, messageId: string) => void;
    stopStreaming: (threadId?: string | null) => void;
    handleSend: () => void;
    handleKeyDown: (event: React.KeyboardEvent) => void;
    canStartGoalRun: boolean;
    startGoalRunFromPrompt: (text: string) => Promise<boolean>;
    tabItems: Array<{ id: AgentChatPanelView; label: string; count: number | null }>;
};

const AgentChatPanelRuntimeContext = createContext<AgentChatPanelRuntimeValue | null>(null);

export function AgentChatPanelProvider({ children }: { children?: React.ReactNode }) {
    const open = useWorkspaceStore((s) => s.agentPanelOpen);
    const togglePanel = useWorkspaceStore((s) => s.toggleAgentPanel);
    const activePaneId = useWorkspaceStore((s) => s.activePaneId());
    const activeWorkspace = useWorkspaceStore((s) => s.activeWorkspace());

    const threads = useAgentStore((s) => s.threads);
    const activeThreadId = useAgentStore((s) => s.activeThreadId);
    const createThread = useAgentStore((s) => s.createThread);
    const deleteThread = useAgentStore((s) => s.deleteThread);
    const setActiveThread = useAgentStore((s) => s.setActiveThread);
    const addMessage = useAgentStore((s) => s.addMessage);
    const deleteMessage = useAgentStore((s) => s.deleteMessage);
    const updateLastAssistantMessage = useAgentStore((s) => s.updateLastAssistantMessage);
    const setThreadTodos = useAgentStore((s) => s.setThreadTodos);
    const setThreadDaemonId = useAgentStore((s) => s.setThreadDaemonId);
    const agentSettings = useAgentStore((s) => s.agentSettings);
    const agentSettingsHydrated = useAgentStore((s) => s.agentSettingsHydrated);
    const agentSettingsDirty = useAgentStore((s) => s.agentSettingsDirty);
    const markAgentSettingsSynced = useAgentStore((s) => s.markAgentSettingsSynced);
    const updateAgentSetting = useAgentStore((s) => s.updateAgentSetting);
    const searchQuery = useAgentStore((s) => s.searchQuery);
    const setSearchQuery = useAgentStore((s) => s.setSearchQuery);
    const storeMessages = useAgentStore((s) => activeThreadId ? s.messages[activeThreadId] : undefined);
    const storeTodos = useAgentStore((s) => activeThreadId ? s.todos[activeThreadId] : undefined);
    const allMessagesByThread = useAgentStore((s) => s.messages);
    const activeThread = threads.find((thread) => thread.id === activeThreadId);

    const operationalEvents = useAgentMissionStore((s) => s.operationalEvents);
    const cognitiveEvents = useAgentMissionStore((s) => s.cognitiveEvents);
    const contextSnapshots = useAgentMissionStore((s) => s.contextSnapshots);
    const approvals = useAgentMissionStore((s) => s.approvals);
    const memory = useAgentMissionStore((s) => s.memory);
    const updateMemory = useAgentMissionStore((s) => s.updateMemory);
    const historySummary = useAgentMissionStore((s) => s.historySummary);
    const historyHits = useAgentMissionStore((s) => s.historyHits);
    const symbolHits = useAgentMissionStore((s) => s.symbolHits);
    const snippets = useSnippetStore((s) => s.snippets);
    const transcripts = useTranscriptStore((s) => s.transcripts);
    const addNotification = useNotificationStore((s) => s.addNotification);

    const [input, setInput] = useState("");
    const [view, setView] = useState<AgentChatPanelView>("threads");
    const [chatBackView, setChatBackView] = useState<AgentChatPanelView>("threads");
    const [historyQuery, setHistoryQuery] = useState("");
    const [symbolQuery, setSymbolQuery] = useState("");
    const [daemonTodosByThread, setDaemonTodosByThread] = useState<Record<string, AgentTodoItem[]>>({});
    const [goalRunsForTrace, setGoalRunsForTrace] = useState<GoalRun[]>([]);
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const abortRef = useRef<AbortController | null>(null);
    // Daemon mode: track the daemon's thread ID (for conversation continuity)
    // and which local frontend thread should receive daemon events.
    const daemonThreadIdRef = useRef<string | null>(null);
    const daemonLocalThreadRef = useRef<string | null>(null);
    // Buffer gateway messages that arrive before thread_created
    const pendingGatewayMessagesRef = useRef<Array<{ role: "user"; content: string; inputTokens: number; outputTokens: number; totalTokens: number; isCompactionSummary: boolean }>>([]);
    // Track goal_run_id → workspaceId for auto-cleanup
    const goalRunWorkspacesRef = useRef<Record<string, string>>({});

    // Reset daemon thread refs when backend changes to avoid stale event routing
    useEffect(() => {
        daemonThreadIdRef.current = null;
        daemonLocalThreadRef.current = null;
        setDaemonTodosByThread({});
        setGoalRunsForTrace([]);
        setChatBackView("threads");
    }, [agentSettings.agent_backend]);

    useEffect(() => {
        if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;
        void fetchAllThreadTodos().then(setDaemonTodosByThread);
        void fetchGoalRuns().then(setGoalRunsForTrace);
    }, [agentSettings.agent_backend]);

    useEffect(() => {
        if (!activeThread) {
            daemonThreadIdRef.current = null;
            daemonLocalThreadRef.current = null;
            return;
        }
        daemonLocalThreadRef.current = activeThread.id;
        daemonThreadIdRef.current = activeThread.daemonThreadId ?? null;
    }, [activeThread]);

    useEffect(() => {
        threads.forEach((thread) => {
            if (!thread.daemonThreadId) return;
            const items = daemonTodosByThread[thread.daemonThreadId];
            if (!items) return;
            setThreadTodos(thread.id, items);
        });
    }, [daemonTodosByThread, setThreadTodos, threads]);

    // Sync provider config to daemon whenever settings change in daemon mode
    useEffect(() => {
        if (!agentSettingsHydrated) return;
        if (!agentSettingsDirty) return;
        if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;
        const amux = getAgentBridge();
        if (!amux?.agentGetConfig || !amux?.agentSetConfigItem) return;

        const nextConfig = buildDaemonAgentConfig(agentSettings);
        void amux.agentGetConfig().then((current: any) => {
            const changes = diffDaemonConfigEntries(current ?? {}, nextConfig);
            if (changes.length === 0) {
                markAgentSettingsSynced();
                return;
            }
            return Promise.all(
                changes.map(({ keyPath, value }) => amux.agentSetConfigItem?.(keyPath, value)),
            ).then(() => {
                markAgentSettingsSynced();
            });
        }).catch(() => {});
    }, [agentSettings, agentSettingsHydrated, agentSettingsDirty, markAgentSettingsSynced]);

    // Subscribe to daemon agent events when in daemon or external agent mode
    useEffect(() => {
        if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;

        const amux = getAgentBridge();
        if (!amux?.onAgentEvent) return;

        const unsubscribe = amux.onAgentEvent((event: any) => {
            if (!event?.type) return;

            // Route all daemon events to the local frontend thread
            const tid = daemonLocalThreadRef.current;

            switch (event.type) {
                case "delta": {
                    if (!tid) break;
                    const msgs = useAgentStore.getState().getThreadMessages(tid);
                    const last = msgs[msgs.length - 1];
                    if (last?.role === "assistant" && last.isStreaming) {
                        updateLastAssistantMessage(tid, (last.content || "") + (event.content || ""), true);
                    }
                    break;
                }
                case "reasoning": {
                    if (!tid) break;
                    const rMsgs = useAgentStore.getState().getThreadMessages(tid);
                    const rLast = rMsgs[rMsgs.length - 1];
                    if (rLast?.role === "assistant" && rLast.isStreaming) {
                        updateLastAssistantMessage(tid, rLast.content || "", true, {
                            reasoning: (rLast.reasoning || "") + (event.content || ""),
                        });
                    }
                    break;
                }
                case "done": {
                    if (!tid) break;
                    useAgentMissionStore.getState().setSharedCursorMode("idle");
                    const msgs2 = useAgentStore.getState().getThreadMessages(tid);
                    const last2 = msgs2[msgs2.length - 1];
                    if (last2?.role === "assistant") {
                        updateLastAssistantMessage(tid, last2.content || "(empty)", false, {
                            inputTokens: event.input_tokens ?? 0,
                            outputTokens: event.output_tokens ?? 0,
                            totalTokens: (event.input_tokens ?? 0) + (event.output_tokens ?? 0),
                            provider: event.provider || undefined,
                            model: event.model || undefined,
                            tps: typeof event.tps === "number" ? event.tps : undefined,
                            reasoning: event.reasoning || last2.reasoning || undefined,
                        });
                    }
                    break;
                }
                case "tool_call": {
                    if (!tid) break;
                    // Set cursor mode to agent while tool is executing
                    useAgentMissionStore.getState().setSharedCursorMode("agent");
                    // Finalize the current streaming assistant message before adding tool call
                    const tcMsgs = useAgentStore.getState().getThreadMessages(tid);
                    const tcLast = tcMsgs[tcMsgs.length - 1];
                    if (tcLast?.role === "assistant" && tcLast.isStreaming) {
                        updateLastAssistantMessage(tid, tcLast.content || "Calling tools...", false);
                    }
                    addMessage(tid, {
                        role: "tool",
                        content: "",
                        toolName: event.name,
                        toolCallId: event.call_id,
                        toolArguments: event.arguments,
                        toolStatus: "requested",
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
                        inputTokens: 0,
                        outputTokens: 0,
                        totalTokens: 0,
                        isCompactionSummary: false,
                    });
                    // Add new streaming assistant message for the next LLM turn
                    const isExtAgent = agentSettings.agent_backend === "openclaw" || agentSettings.agent_backend === "hermes";
                    addMessage(tid, {
                        role: "assistant",
                        content: "",
                        provider: isExtAgent ? agentSettings.agent_backend : agentSettings.active_provider,
                        model: isExtAgent ? agentSettings.agent_backend : ((agentSettings[agentSettings.active_provider] as any)?.model || ""),
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
                    updateLastAssistantMessage(tid, `Error: ${event.message}`, false);
                    break;
                }
                case "thread_created": {
                    if (event.thread_id) {
                        // Only update refs if this thread belongs to our current local thread
                        // (goal runners create their own threads that shouldn't hijack the chat)
                        const isOurThread = !daemonLocalThreadRef.current
                            || daemonThreadIdRef.current === null
                            || event.thread_id === daemonThreadIdRef.current;
                        if (isOurThread) {
                            daemonThreadIdRef.current = event.thread_id;
                        }
                        if (daemonLocalThreadRef.current && isOurThread) {
                            setThreadDaemonId(daemonLocalThreadRef.current, event.thread_id);
                        }

                        // Auto-create a local thread if none exists (e.g. gateway message)
                        if (!daemonLocalThreadRef.current) {
                            const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
                            const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
                            const paneId = useWorkspaceStore.getState().activePaneId();
                            const localId = createThread({
                                workspaceId,
                                surfaceId,
                                paneId,
                                title: event.title || "Gateway conversation",
                            });
                            daemonLocalThreadRef.current = localId;
                            setThreadDaemonId(localId, event.thread_id);
                            setActiveThread(localId);
                            setView("chat");
                            if (event.thread_id) {
                                void fetchThreadTodos(event.thread_id).then((items) => {
                                    setThreadTodos(localId, items);
                                    setDaemonTodosByThread((current) => ({ ...current, [event.thread_id]: items }));
                                });
                            }

                            // Flush any buffered gateway messages
                            for (const buffered of pendingGatewayMessagesRef.current) {
                                addMessage(localId, buffered);
                            }
                            pendingGatewayMessagesRef.current = [];

                            // Add streaming assistant placeholder
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
                    }
                    break;
                }
                case "task_update": {
                    const task = event.task;
                    if (!task) break;

                    if (task.status === "awaiting_approval") {
                        // Create an actionable approval record so the user can approve/deny
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

                    break;
                }
                case "goal_run_update":
                case "goal_run_created": {
                    const goalRun = normalizeGoalRun(event.goal_run ?? event.goalRun ?? event.run ?? null);
                    if (!goalRun) break;
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
                        // Auto-cleanup workspace after goal completion
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
                        // Auto-cleanup workspace after goal failure/cancel
                        cleanupGoalRunWorkspace(goalRun.id);
                    }

                    break;
                }
                case "todo_update": {
                    const daemonThreadId = typeof event.thread_id === "string" ? event.thread_id : null;
                    const localThreadId = daemonThreadId && daemonThreadIdRef.current === daemonThreadId
                        ? daemonLocalThreadRef.current
                        : null;
                    if (!Array.isArray(event.items)) break;

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
                                createdAt: typeof item?.created_at === "number"
                                    ? item.created_at
                                    : typeof item?.createdAt === "number"
                                        ? item.createdAt
                                        : null,
                                updatedAt: typeof item?.updated_at === "number"
                                    ? item.updated_at
                                    : typeof item?.updatedAt === "number"
                                        ? item.updatedAt
                                        : null,
                            };
                        })
                        .filter((item: AgentTodoItem | null): item is AgentTodoItem => Boolean(item));
                    if (daemonThreadId) {
                        setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: todos }));
                    }
                    if (localThreadId) {
                        setThreadTodos(localThreadId, todos);
                    }
                    break;
                }
                case "workflow_notice": {
                    recordDaemonWorkflowNotice(event);
                    break;
                }
                case "workspace_command": {
                    // Execute workspace mutation on the frontend
                    const cmd = event.command;
                    const cmdArgs = event.args || {};
                    const ws = useWorkspaceStore.getState();
                    try {
                        switch (cmd) {
                            case "create_workspace": ws.createWorkspace(cmdArgs.name); break;
                            case "set_active_workspace": {
                                const w = ws.workspaces.find((x: any) => x.id === cmdArgs.workspace || x.name === cmdArgs.workspace);
                                if (w) ws.setActiveWorkspace(w.id);
                                break;
                            }
                            case "create_surface": ws.createSurface(undefined, undefined); break;
                            case "set_active_surface": {
                                const aw = ws.activeWorkspace();
                                if (aw) {
                                    const s = aw.surfaces.find((x: any) => x.id === cmdArgs.surface || x.name === cmdArgs.surface);
                                    if (s) ws.setActiveSurface(s.id);
                                }
                                break;
                            }
                            case "split_pane": ws.splitActive(cmdArgs.direction === "vertical" ? "vertical" : "horizontal"); break;
                            case "rename_pane": {
                                const paneId = cmdArgs.pane || ws.activePaneId();
                                if (paneId) ws.setPaneName(paneId, cmdArgs.name);
                                break;
                            }
                            case "attach_agent_terminal": {
                                if (typeof cmdArgs.workspace_id === "string" && cmdArgs.workspace_id) {
                                    void provisionTerminalPaneInWorkspace({
                                        workspaceId: cmdArgs.workspace_id,
                                        paneName: typeof cmdArgs.pane_name === "string" && cmdArgs.pane_name
                                            ? cmdArgs.pane_name
                                            : "Work",
                                        cwd: typeof cmdArgs.cwd === "string" && cmdArgs.cwd ? cmdArgs.cwd : null,
                                        sessionId: typeof cmdArgs.session_id === "string" && cmdArgs.session_id
                                            ? cmdArgs.session_id
                                            : null,
                                    });
                                }
                                break;
                            }
                            case "set_layout_preset": break; // TODO
                            case "equalize_layout": break; // TODO
                            case "create_snippet": {
                                const snippetStore = useSnippetStore.getState();
                                snippetStore.addSnippet({ name: cmdArgs.name, content: cmdArgs.content, category: cmdArgs.category, description: cmdArgs.description, tags: cmdArgs.tags, owner: "assistant" });
                                break;
                            }
                            case "run_snippet": break; // TODO
                        }
                    } catch (e: any) {
                        console.warn("workspace command failed:", cmd, e);
                    }
                    break;
                }
                case "concierge_welcome": {
                    useAgentStore.setState({
                        conciergeWelcome: {
                            content: event.content ?? "",
                            actions: event.actions ?? [],
                        }
                    });
                    break;
                }
                case "gateway_incoming": {
                    const gwMsg = {
                        role: "user" as const,
                        content: `[${event.platform} — ${event.sender}]: ${event.content}`,
                        inputTokens: 0,
                        outputTokens: 0,
                        totalTokens: 0,
                        isCompactionSummary: false,
                    };

                    const inTid = daemonLocalThreadRef.current;
                    if (inTid) {
                        // Thread exists — add message directly + streaming placeholder
                        addMessage(inTid, gwMsg);
                        addMessage(inTid, {
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
                    } else {
                        // Thread not yet created — buffer for thread_created handler
                        pendingGatewayMessagesRef.current.push(gwMsg);
                    }
                    break;
                }
            }
        });

        return unsubscribe;
    }, [agentSettings.agent_backend, addMessage, updateLastAssistantMessage, setActiveThread, setThreadDaemonId, setThreadTodos]);

    const messages = storeMessages ?? EMPTY_MESSAGES;
    const todos = storeTodos ?? [];

    useEffect(() => {
        const daemonThreadId = daemonThreadIdRef.current;
        const localThreadId = daemonLocalThreadRef.current;
        if (!daemonThreadId || !localThreadId || localThreadId !== activeThreadId) return;
        void fetchThreadTodos(daemonThreadId).then((items) => {
            setThreadTodos(localThreadId, items);
            setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: items }));
        });
    }, [activeThreadId, setThreadTodos]);
    const scopePaneId = activeThread?.paneId ?? activePaneId;
    const pendingApprovals = approvals.filter((approval) => approval.status === "pending");
    const scopeController = getTerminalController(scopePaneId);

    const usageMessageCount = useMemo(
        () => Object.values(allMessagesByThread)
            .flat()
            .filter((message) => message.role === "assistant" && ((message.totalTokens ?? 0) > 0 || message.cost !== undefined)).length,
        [allMessagesByThread],
    );

    const scopedOperationalEvents = useMemo(() => {
        if (!scopePaneId) return operationalEvents.slice(0, 30);
        return operationalEvents.filter((event) => event.paneId === scopePaneId).slice(0, 30);
    }, [operationalEvents, scopePaneId]);

    const scopedCognitiveEvents = useMemo(() => {
        if (!scopePaneId) return cognitiveEvents.slice(0, 20);
        return cognitiveEvents.filter((event) => event.paneId === scopePaneId).slice(0, 20);
    }, [cognitiveEvents, scopePaneId]);

    const latestContextSnapshot = useMemo(() => {
        if (!scopePaneId) return contextSnapshots[0];
        return contextSnapshots.find((snapshot) => snapshot.paneId === scopePaneId) ?? contextSnapshots[0];
    }, [contextSnapshots, scopePaneId]);

    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages.length]);

    useEffect(() => {
        if (open && activeThreadId) {
            setView("chat");
            setTimeout(() => inputRef.current?.focus(), 100);
        } else if (open) {
            setView("threads");
        }
    }, [open, activeThreadId]);

    const filteredThreads = searchQuery
        ? threads.filter(
            (thread) => thread.title.toLowerCase().includes(searchQuery.toLowerCase())
                || thread.lastMessagePreview.toLowerCase().includes(searchQuery.toLowerCase()),
        )
        : threads;

    const isStreamingResponse = messages.some((message) => message.role === "assistant" && message.isStreaming);

    function recordDaemonWorkflowNotice(event: any) {
        const daemonThreadId = typeof event?.thread_id === "string" ? event.thread_id : null;
        const localThreadId = daemonThreadId && daemonThreadIdRef.current === daemonThreadId
            ? daemonLocalThreadRef.current
            : daemonLocalThreadRef.current;
        const thread = localThreadId
            ? useAgentStore.getState().threads.find((entry) => entry.id === localThreadId)
            : undefined;
        const paneId = thread?.paneId ?? activePaneId ?? "agent";
        const workspaceId = thread?.workspaceId ?? activeWorkspace?.id ?? null;
        const surfaceId = thread?.surfaceId ?? activeWorkspace?.surfaces?.[0]?.id ?? null;
        const kind = typeof event?.kind === "string" ? event.kind : "tool-call";
        const message = typeof event?.message === "string" ? event.message : null;
        const details = typeof event?.details === "string" ? event.details : null;

        if (kind === "transport-fallback" && details) {
            try {
                const parsed = JSON.parse(details);
                const provider = typeof parsed?.provider === "string" ? parsed.provider : null;
                const toTransport = parsed?.to === "chat_completions" ? "chat_completions" : null;
                if (provider && toTransport) {
                    const currentSettings = useAgentStore.getState().agentSettings;
                    const currentConfig = currentSettings[provider as keyof typeof currentSettings];
                    if (currentConfig && typeof currentConfig === "object" && "base_url" in currentConfig) {
                        useAgentStore.getState().updateAgentSetting(
                            provider as keyof typeof currentSettings,
                            {
                                ...(currentConfig as AgentProviderConfig),
                                api_transport: toTransport,
                            } as any,
                        );
                    }
                }
            } catch {
                // leave notice recording best-effort
            }
        }

        useAgentMissionStore.getState().recordOperationalEvent({
            paneId,
            workspaceId,
            surfaceId,
            sessionId: daemonThreadId,
            kind: kind as any,
            command: kind,
            message: details ? `${message ?? ""}${message ? "\n" : ""}${details}` : message,
        });
    }

    function stopStreaming(threadId?: string | null) {
        const targetThreadId = threadId ?? activeThreadId;
        if (!targetThreadId) return;

        if (shouldUseDaemonRuntime(agentSettings.agent_backend)) {
            const amux = getAgentBridge();
            const daemonTid = daemonThreadIdRef.current;
            if (daemonTid && amux?.agentStopStream) {
                void amux.agentStopStream(daemonTid);
            }
        }

        abortThreadStream(targetThreadId);
        if (abortRef.current) {
            abortRef.current.abort();
            abortRef.current = null;
        }

        const threadMessages = useAgentStore.getState().getThreadMessages(targetThreadId);
        const lastMessage = threadMessages[threadMessages.length - 1];
        if (lastMessage?.role === "assistant" && lastMessage.isStreaming) {
            updateLastAssistantMessage(targetThreadId, lastMessage.content || "(stopped)", false);
        }
    }

    function cleanupGoalRunWorkspace(goalRunId: string) {
        const wsId = goalRunWorkspacesRef.current[goalRunId];
        if (!wsId) return;
        // Delay cleanup slightly to let final events flush
        setTimeout(() => {
            const store = useWorkspaceStore.getState();
            if (store.workspaces.some((ws) => ws.id === wsId)) {
                console.log("[agent] auto-closing workspace for finished goal run", { goalRunId, wsId });
                store.closeWorkspace(wsId);
            }
            delete goalRunWorkspacesRef.current[goalRunId];
        }, 3000);
    }

    function sendMessage(text: string) {
        if (!text) return;

        if (shouldUseDaemonRuntime(agentSettings.agent_backend)) {
            const amux = getAgentBridge();
            if (!amux?.agentSendMessage) {
                sendMessageLegacy(text);
                return;
            }
            void (async () => {
                const daemonTid = daemonThreadIdRef.current;
                let threadId = activeThreadId;
                console.log("[agent-send] start", { daemonTid, activeThreadId: threadId, daemonLocalRef: daemonLocalThreadRef.current });

                // Reuse existing thread refs — never create a new thread if we have one
                if (!threadId && daemonLocalThreadRef.current) {
                    threadId = daemonLocalThreadRef.current;
                    console.log("[agent-send] reused daemonLocalThreadRef", threadId);
                }

                // Only create a new thread if we truly have nothing
                if (!threadId) {
                    const provision = await provisionAgentWorkspaceTerminals({
                        title: text.slice(0, 50) || "Agent Conversation",
                        cwd: activeWorkspace?.cwd ?? null,
                    });
                    threadId = createThread({
                        workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
                        surfaceId: provision?.surfaceId ?? activeWorkspace?.surfaces?.[0]?.id ?? null,
                        paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
                        title: text.slice(0, 50),
                    });
                    setView("chat");
                    console.log("[agent-send] created new thread", threadId);
                }

                // Always ensure we have a terminal session (provision if needed)
                let thread = useAgentStore.getState().threads.find((t) => t.id === threadId);
                let preferredSessionId = thread?.paneId ? resolvePaneSessionId(thread.paneId) : null;
                if (!preferredSessionId && thread?.workspaceId) {
                    // Thread already has a workspace — provision a terminal pane
                    // inside the existing workspace instead of creating a new one.
                    const pane = await provisionTerminalPaneInWorkspace({
                        workspaceId: thread.workspaceId,
                        paneName: "Coordinator",
                        cwd: activeWorkspace?.cwd ?? null,
                        reusePrimaryPane: true,
                    });
                    preferredSessionId = pane?.sessionId ?? null;
                } else if (!preferredSessionId) {
                    // No workspace at all — first message, create everything.
                    const provision = await provisionAgentWorkspaceTerminals({
                        title: thread?.title || text.slice(0, 50) || "Agent Conversation",
                        cwd: activeWorkspace?.cwd ?? null,
                    });
                    preferredSessionId = provision?.coordinatorSessionId ?? null;
                }

                // Make sure thread is active
                if (useAgentStore.getState().activeThreadId !== threadId) {
                    setActiveThread(threadId);
                }

                if (!threadId) {
                    return;
                }

                addMessage(threadId, {
                    role: "user",
                    content: text,
                    inputTokens: 0,
                    outputTokens: 0,
                    totalTokens: 0,
                    isCompactionSummary: false,
                });

                const isExternalAgent = agentSettings.agent_backend === "openclaw" || agentSettings.agent_backend === "hermes";
                addMessage(threadId, {
                    role: "assistant",
                    content: "",
                    provider: isExternalAgent ? agentSettings.agent_backend : agentSettings.active_provider,
                    model: isExternalAgent ? agentSettings.agent_backend : ((agentSettings[agentSettings.active_provider] as any)?.model || "unknown"),
                    inputTokens: 0,
                    outputTokens: 0,
                    totalTokens: 0,
                    isCompactionSummary: false,
                    isStreaming: true,
                });

                daemonLocalThreadRef.current = threadId;

                // Always send conversation context so the daemon can seed the thread
                // if it was lost (e.g. daemon restart). The daemon ignores context
                // if the thread already has messages in memory.
                let contextMessages: unknown[] | undefined;
                {
                    const existingMsgs = useAgentStore.getState().getThreadMessages(threadId);
                    // Exclude the just-added user message and streaming assistant placeholder
                    const historyMsgs = existingMsgs.filter(
                        (m) => !m.isStreaming && !m.isCompactionSummary
                    ).slice(0, -1); // remove the user message we just added
                    if (historyMsgs.length > 0) {
                        contextMessages = historyMsgs.map((m, idx) => ({
                            id: `${threadId}:ctx:${idx}`,
                            thread_id: threadId,
                            created_at: m.createdAt ?? Date.now(),
                            role: m.role,
                            content: m.content,
                            provider: m.provider ?? null,
                            model: m.model ?? null,
                            input_tokens: m.inputTokens ?? 0,
                            output_tokens: m.outputTokens ?? 0,
                            total_tokens: m.totalTokens ?? 0,
                            reasoning: m.reasoning ?? null,
                            tool_calls_json: m.toolCalls ? JSON.stringify(m.toolCalls) : null,
                            metadata_json: m.toolName ? JSON.stringify({
                                toolCallId: m.toolCallId,
                                toolName: m.toolName,
                                toolArguments: m.toolArguments,
                                toolStatus: m.toolStatus,
                            }) : null,
                        }));
                    }
                }
                console.log("[agent-send] sending", {
                    daemonTid,
                    threadId,
                    contextCount: contextMessages?.length ?? 0,
                    contextRoles: contextMessages?.map((m: any) => m.role),
                });
                // Use daemonTid if available, otherwise pass local threadId so
                // the daemon uses the same ID — prevents duplicate threads in SQLite.
                await amux.agentSendMessage!(daemonTid || threadId, text, preferredSessionId, contextMessages);
            })();
            return;
        }

        // Legacy mode: run LLM in frontend
        sendMessageLegacy(text);
    }

    function sendMessageLegacy(text: string) {
        let threadId = activeThreadId;
        if (!threadId) {
            const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
            const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
            const paneId = useWorkspaceStore.getState().activePaneId();
            threadId = createThread({
                workspaceId,
                surfaceId,
                paneId,
                title: text.slice(0, 50),
            });
            setView("chat");
        }

        addMessage(threadId, {
            role: "user",
            content: text,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
        });

        const providerConfig = agentSettings[agentSettings.active_provider] as AgentProviderConfig;
        const currentThreadId = threadId;
        const tools = getAvailableTools({
            enable_bash_tool: agentSettings.enable_bash_tool,
            gateway_enabled: agentSettings.gateway_enabled,
            enable_vision_tool: agentSettings.enable_vision_tool,
            enable_web_browsing_tool: agentSettings.enable_web_browsing_tool,
        });
        const toolCapabilities = getToolCapabilityDescription(tools);
        const system_prompt = agentSettings.system_prompt + toolCapabilities;

        stopStreaming(currentThreadId);

        addMessage(currentThreadId, {
            role: "assistant",
            content: "",
            provider: agentSettings.active_provider,
            model: providerConfig.model,
            api_transport: providerConfig.api_transport,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
            isStreaming: true,
        });
        const controller = new AbortController();
        abortRef.current = controller;
        setThreadAbortController(currentThreadId, controller);

        (async () => {
        const configuredToolLoops = Number(agentSettings.max_tool_loops ?? 0);
        const max_tool_loops = Number.isFinite(configuredToolLoops) && configuredToolLoops > 0
                ? Math.min(1000, configuredToolLoops)
                : Infinity;
            let loopCount = 0;
            let allCurrentMessages = useAgentStore.getState().getThreadMessages(currentThreadId);
            await syncMessagesToHoncho(agentSettings, currentThreadId, allCurrentMessages);
            const getCurrentProviderConfig = () => (
                useAgentStore.getState().agentSettings[agentSettings.active_provider] as AgentProviderConfig
            );
            const updateThreadUpstreamState = (upstreamThreadId?: string) => {
                useAgentStore.setState((state) => ({
                    threads: state.threads.map((thread) => thread.id === currentThreadId ? {
                        ...thread,
                        upstreamThreadId: upstreamThreadId ?? null,
                        upstreamTransport: preparedRequest.transport,
                        upstreamProvider: agentSettings.active_provider,
                        upstreamModel: getCurrentProviderConfig().model,
                        upstreamAssistantId: getCurrentProviderConfig().assistant_id || null,
                    } : thread),
                }));
            };
            const getContextSettings = () => ({
                ...useAgentStore.getState().agentSettings,
                context_window_tokens: getEffectiveContextWindow(
                    agentSettings.active_provider,
                    getCurrentProviderConfig(),
                ),
            });
            let preparedRequest = prepareOpenAIRequest(
                allCurrentMessages.slice(0, -1),
                getContextSettings(),
                agentSettings.active_provider,
                getCurrentProviderConfig().model,
                getCurrentProviderConfig().api_transport,
                getCurrentProviderConfig().auth_source,
                getCurrentProviderConfig().assistant_id,
                useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId),
            );
            let lastPersistedReasoning: string | null = null;
            const honchoContext = await buildHonchoContext(agentSettings, currentThreadId, text);
            const effectiveSystemPrompt = honchoContext
                ? `${system_prompt}\n\nCross-session memory:\n${honchoContext}`
                : system_prompt;

            const persistReasoningTrace = (reasoning: string) => {
                const normalized = reasoning.trim();
                if (!normalized) return;
                if (normalized === lastPersistedReasoning) return;

                const thread = useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId);
                const paneId = thread?.paneId ?? useWorkspaceStore.getState().activePaneId() ?? "agent";
                const workspaceId = thread?.workspaceId ?? useWorkspaceStore.getState().activeWorkspaceId;
                const surfaceId = thread?.surfaceId ?? useWorkspaceStore.getState().activeSurface()?.id ?? null;

                useAgentMissionStore.getState().recordCognitiveOutput({
                    paneId,
                    workspaceId,
                    surfaceId,
                    sessionId: null,
                    text: `<INNER_MONOLOGUE>\n${normalized}\n</INNER_MONOLOGUE>`,
                });
                lastPersistedReasoning = normalized;
            };

            while (loopCount < max_tool_loops) {
                loopCount += 1;
                let accumulated = "";
                let accumulatedReasoning = "";
                const responseStartedAt = Date.now();
                let receivedToolCalls = false;
                let roundToolCalls: Array<{ id: string; type: "function"; function: { name: string; arguments: string } }> = [];

                try {
                    for await (const chunk of sendChatCompletion({
                        provider: agentSettings.active_provider,
                        config: {
                            ...providerConfig,
                            api_transport: preparedRequest.transport,
                        },
                        system_prompt: effectiveSystemPrompt,
                        messages: preparedRequest.messages,
                        streaming: agentSettings.enable_streaming,
                        signal: controller.signal,
                        tools: tools.length > 0 ? tools : undefined,
                        reasoning_effort: agentSettings.reasoning_effort,
                        previousResponseId: preparedRequest.previousResponseId,
                        upstreamThreadId: preparedRequest.upstreamThreadId,
                    })) {
                        if (chunk.type === "delta") {
                            accumulated += chunk.content;
                            if (chunk.reasoning) accumulatedReasoning += chunk.reasoning;
                            updateLastAssistantMessage(currentThreadId, accumulated, true, {
                                reasoning: accumulatedReasoning || undefined,
                            });
                            continue;
                        }

                        if (chunk.type === "done") {
                            if (chunk.content && chunk.content !== accumulated) accumulated = chunk.content;
                            if (chunk.reasoning) accumulatedReasoning = chunk.reasoning;

                            persistReasoningTrace(accumulatedReasoning);

                            const elapsedSeconds = Math.max(0.001, (Date.now() - responseStartedAt) / 1000);
                            const outputTokens = Number(chunk.outputTokens ?? 0);
                            const inputTokens = Number(chunk.inputTokens ?? 0);
                            const totalTokens = Number(chunk.totalTokens ?? (inputTokens + outputTokens));
                            const tps = outputTokens > 0 ? outputTokens / elapsedSeconds : undefined;

                            updateLastAssistantMessage(currentThreadId, accumulated || "(empty response)", false, {
                                inputTokens,
                                outputTokens,
                                totalTokens,
                                reasoning: accumulatedReasoning || undefined,
                                reasoningTokens: chunk.reasoningTokens,
                                audioTokens: chunk.audioTokens,
                                videoTokens: chunk.videoTokens,
                                cost: chunk.cost,
                                tps,
                                api_transport: preparedRequest.transport,
                                responseId: chunk.responseId,
                            });
                            updateThreadUpstreamState(chunk.upstreamThreadId);
                            continue;
                        }

                        if (chunk.type === "error") {
                            updateLastAssistantMessage(currentThreadId, `Error: ${chunk.content}`, false);
                            continue;
                        }

                        if (chunk.type === "transport_fallback") {
                            useAgentStore.getState().updateAgentSetting(agentSettings.active_provider as keyof ReturnType<typeof useAgentStore.getState>["agentSettings"], {
                                ...providerConfig,
                                api_transport: "chat_completions",
                            } as any);
                            preparedRequest = { ...preparedRequest, transport: "chat_completions", previousResponseId: undefined, upstreamThreadId: undefined };
                            updateThreadUpstreamState(undefined);
                            continue;
                        }

                        if (chunk.type === "tool_calls" && chunk.toolCalls) {
                            receivedToolCalls = true;
                            roundToolCalls = chunk.toolCalls;
                            if (chunk.reasoning) accumulatedReasoning = chunk.reasoning;
                            if (chunk.content) accumulated = chunk.content;

                            persistReasoningTrace(accumulatedReasoning);

                            updateLastAssistantMessage(currentThreadId, accumulated || "Calling tools...", false, {
                                reasoning: accumulatedReasoning || undefined,
                                inputTokens: Number(chunk.inputTokens ?? 0),
                                outputTokens: Number(chunk.outputTokens ?? 0),
                                totalTokens: Number(chunk.totalTokens ?? ((chunk.inputTokens ?? 0) + (chunk.outputTokens ?? 0))),
                                reasoningTokens: chunk.reasoningTokens,
                                audioTokens: chunk.audioTokens,
                                videoTokens: chunk.videoTokens,
                                cost: chunk.cost,
                                toolCalls: roundToolCalls,
                                api_transport: preparedRequest.transport,
                                responseId: chunk.responseId,
                            });
                            updateThreadUpstreamState(chunk.upstreamThreadId);

                            for (const toolCall of chunk.toolCalls) {
                                addMessage(currentThreadId, {
                                    role: "tool",
                                    content: "",
                                    toolName: toolCall.function.name,
                                    toolCallId: toolCall.id,
                                    toolArguments: toolCall.function.arguments,
                                    toolStatus: "requested",
                                    inputTokens: 0,
                                    outputTokens: 0,
                                    totalTokens: 0,
                                    isCompactionSummary: false,
                                });
                            }

                            const toolResults = [];
                            for (const toolCall of chunk.toolCalls) {
                                useAgentMissionStore.getState().recordToolCall({
                                    toolName: toolCall.function.name,
                                    arguments: toolCall.function.arguments,
                                });

                                const result = await executeTool(toolCall);
                                toolResults.push(result);

                                addMessage(currentThreadId, {
                                    role: "tool",
                                    content: result.content,
                                    toolName: result.name,
                                    toolCallId: result.toolCallId,
                                    toolArguments: toolCall.function.arguments,
                                    toolStatus: result.content.startsWith("Error:") ? "error" : "done",
                                    inputTokens: 0,
                                    outputTokens: 0,
                                    totalTokens: 0,
                                    isCompactionSummary: false,
                                });
                            }

                            updateLastAssistantMessage(currentThreadId, accumulated || "Tools executed.", false);

                            addMessage(currentThreadId, {
                                role: "assistant",
                                content: "",
                                provider: agentSettings.active_provider,
                                model: providerConfig.model,
                                api_transport: preparedRequest.transport,
                                inputTokens: 0,
                                outputTokens: 0,
                                totalTokens: 0,
                                isCompactionSummary: false,
                                isStreaming: true,
                            });
                        }
                    }
                } catch (error: any) {
                    if (error.name !== "AbortError") {
                        updateLastAssistantMessage(currentThreadId, `Error: ${error.message || String(error)}`);
                    }
                    break;
                }

                if (!receivedToolCalls) break;
                allCurrentMessages = useAgentStore.getState().getThreadMessages(currentThreadId);
                preparedRequest = prepareOpenAIRequest(
                    allCurrentMessages.slice(0, -1),
                    getContextSettings(),
                    agentSettings.active_provider,
                    getCurrentProviderConfig().model,
                    getCurrentProviderConfig().api_transport,
                    getCurrentProviderConfig().auth_source,
                    getCurrentProviderConfig().assistant_id,
                    useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId),
                );
            }

            await syncMessagesToHoncho(
                agentSettings,
                currentThreadId,
                useAgentStore.getState().getThreadMessages(currentThreadId),
            );

            if (Number.isFinite(max_tool_loops) && loopCount >= max_tool_loops) {
                updateLastAssistantMessage(currentThreadId, "(Tool execution limit reached)", false);
            }

            if (abortRef.current === controller) {
                abortRef.current = null;
            }
            clearThreadAbortController(currentThreadId, controller);
        })();
    }

    function handleSend() {
        const text = input.trim();
        if (!text) return;
        sendMessage(text);
        setInput("");
    }

    function handleKeyDown(event: React.KeyboardEvent) {
        if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            handleSend();
        }
    }

    async function startGoalRunFromPrompt(text: string): Promise<boolean> {
        const goal = text.trim();
        if (!goal || !goalRunSupportAvailable()) {
            return false;
        }

        // Ensure we have a local thread for the goal run
        let threadId = activeThreadId;
        if (!threadId && daemonLocalThreadRef.current) {
            threadId = daemonLocalThreadRef.current;
            setActiveThread(threadId);
        }
        if (!threadId) {
            const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
            const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
            const paneId = useWorkspaceStore.getState().activePaneId();
            threadId = createThread({
                workspaceId,
                surfaceId,
                paneId,
                title: goal.slice(0, 50),
            });
        }

        const provision = await provisionAgentWorkspaceTerminals({
            title: goal,
            cwd: activeWorkspace?.cwd ?? null,
        });

        // Use daemonTid if available, otherwise pass local threadId
        const effectiveThreadId = daemonThreadIdRef.current || threadId;
        daemonLocalThreadRef.current = threadId;

        // Add user goal as a message so it shows in the chat thread
        addMessage(threadId, {
            role: "user",
            content: goal,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
        });

        // Set this thread as active and switch to chat view
        setActiveThread(threadId);

        const run = await startGoalRun({
            goal,
            title: goal.slice(0, 72),
            priority: "normal",
            threadId: effectiveThreadId,
            sessionId: provision?.coordinatorSessionId ?? null,
        });

        // Track goal run → workspace for auto-cleanup
        if (run?.id && provision?.workspaceId) {
            goalRunWorkspacesRef.current[run.id] = provision.workspaceId;
        }

        if (!run) {
            addNotification({
                title: "Goal runner unavailable",
                body: "Could not start long-running goal",
                subtitle: "Backend goal-run IPC is not available yet.",
                icon: "alert-triangle",
                source: "system",
                workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
                paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
                panelId: provision?.coordinatorPaneId ?? activePaneId ?? null,
            });
            return false;
        }

        addNotification({
            title: "Goal runner started",
            body: run.title,
            subtitle: run.plan_summary || "The daemon is planning the run.",
            icon: "sparkles",
            source: "system",
            workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
            paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
            panelId: provision?.coordinatorPaneId ?? activePaneId ?? null,
        });
        setView("tasks");
        return true;
    }

    const tabItems = [
        { id: "threads", label: "Threads", count: threads.length },
        { id: "chat", label: "Chat", count: null },
        { id: "trace", label: "Trace", count: scopedCognitiveEvents.length },
        { id: "usage", label: "Usage", count: usageMessageCount },
        { id: "context", label: "Context", count: null },
        { id: "graph", label: "Graph", count: null },
        { id: "coding-agents", label: "Coding Agents", count: null },
        { id: "ai-training", label: "AI Training", count: null },
        { id: "tasks", label: "Tasks", count: null },
        { id: "subagents", label: "Subagents", count: null },
    ] satisfies Array<{ id: AgentChatPanelView; label: string; count: number | null }>;

    const value = useMemo<AgentChatPanelRuntimeValue>(() => ({
        togglePanel,
        activeWorkspace,
        threads,
        activeThread,
        activeThreadId,
        createThread,
        deleteThread,
        setActiveThread,
        agentSettings,
        updateAgentSetting,
        searchQuery,
        setSearchQuery,
        messages,
        todos,
        daemonTodosByThread,
        goalRunsForTrace,
        allMessagesByThread,
        pendingApprovals,
        scopedOperationalEvents,
        scopedCognitiveEvents,
        latestContextSnapshot,
        memory,
        updateMemory,
        historySummary,
        historyHits,
        symbolHits,
        snippets,
        transcripts,
        scopePaneId,
        scopeController,
        input,
        setInput,
        historyQuery,
        setHistoryQuery,
        symbolQuery,
        setSymbolQuery,
        view,
        setView,
        chatBackView,
        setChatBackView,
        usageMessageCount,
        filteredThreads,
        isStreamingResponse,
        messagesEndRef,
        inputRef,
        sendMessage,
        deleteMessage,
        stopStreaming,
        handleSend,
        handleKeyDown,
        canStartGoalRun: goalRunSupportAvailable(),
        startGoalRunFromPrompt,
        tabItems,
    }), [
        togglePanel,
        activeWorkspace,
        threads,
        activeThread,
        activeThreadId,
        createThread,
        deleteThread,
        setActiveThread,
        agentSettings,
        updateAgentSetting,
        searchQuery,
        setSearchQuery,
        messages,
        todos,
        daemonTodosByThread,
        allMessagesByThread,
        pendingApprovals,
        scopedOperationalEvents,
        scopedCognitiveEvents,
        latestContextSnapshot,
        memory,
        updateMemory,
        historySummary,
        historyHits,
        symbolHits,
        snippets,
        transcripts,
        scopePaneId,
        scopeController,
        input,
        historyQuery,
        symbolQuery,
        view,
        chatBackView,
        usageMessageCount,
        filteredThreads,
        isStreamingResponse,
        deleteMessage,
        startGoalRunFromPrompt,
        tabItems,
    ]);

    if (!open) {
        return null;
    }

    return (
        <AgentChatPanelRuntimeContext.Provider value={value}>
            {children}
        </AgentChatPanelRuntimeContext.Provider>
    );
}

export function useAgentChatPanelRuntime(): AgentChatPanelRuntimeValue {
    const runtime = useContext(AgentChatPanelRuntimeContext);
    if (!runtime) {
        throw new Error("AgentChatPanel runtime is only available inside AgentChatPanelProvider.");
    }
    return runtime;
}

export function AgentChatPanelScaffold({ style, className }: { style?: CSSProperties; className?: string }) {
    return (
        <div
            style={{
                width: 560,
                minWidth: 380,
                maxWidth: 820,
                height: "100%",
                display: "flex",
                flexDirection: "column",
                background: "var(--bg-primary)",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius-xl)",
                overflow: "hidden",
                ...(style ?? {}),
            }}
            className={className}
        >
            <AgentChatPanelHeader />
            <AgentChatPanelTabs />
            <div style={{ flex: 1, overflow: "hidden", position: "relative", display: "flex", flexDirection: "column" }}>
                <AgentChatPanelCurrentSurface />
            </div>
        </div>
    );
}

export function AgentChatPanelHeader() {
    const runtime = useAgentChatPanelRuntime();
    const { view, activeThread, setActiveThread, setView, chatBackView, setChatBackView, togglePanel, createThread } = runtime;

    return (
        <div
            style={{
                padding: "var(--space-4)",
                borderBottom: "1px solid var(--border)",
                flexShrink: 0,
                background: "var(--bg-secondary)",
            }}
        >
            <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                        {view === "chat" && activeThread && (
                            <button
                                onClick={() => {
                                    setActiveThread(null);
                                    setView(chatBackView);
                                }}
                                style={iconButtonStyle}
                                title="Back to threads"
                            >
                                ←
                            </button>
                        )}
                        <span
                            className="amux-agent-indicator"
                            style={{ background: "var(--mission-soft)", borderColor: "var(--mission-glow)", color: "var(--mission)" }}
                        >
                            Mission Console
                        </span>
                    </div>

                    <span style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>
                        {view === "threads" ? "Live Intelligence Surfaces" : activeThread?.title ?? "Conversation Lane"}
                    </span>

                    <span style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)" }}>
                        Reasoning, execution context, recall memory, and symbols
                    </span>
                </div>

                <div style={{ display: "flex", gap: "var(--space-1)" }}>
                    <button
                        onClick={() => {
                            const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
                            createThread({ workspaceId });
                            setChatBackView("threads");
                            setView("chat");
                        }}
                        style={iconButtonStyle}
                        title="New thread"
                    >
                        +
                    </button>
                    <button onClick={togglePanel} style={iconButtonStyle} title="Close">
                        ✕
                    </button>
                </div>
            </div>
        </div>
    );
}

export function AgentChatPanelTabs() {
    const { tabItems, view, setView } = useAgentChatPanelRuntime();

    return (
        <div
            style={{
                display: "flex",
                gap: "var(--space-1)",
                padding: "var(--space-2) var(--space-3)",
                borderBottom: "1px solid var(--border)",
                background: "var(--bg-secondary)",
                overflowX: "auto",
            }}
        >
            {tabItems.map((tab) => (
                <button
                    key={tab.id}
                    type="button"
                    onClick={() => setView(tab.id)}
                    style={{
                        padding: "var(--space-1) var(--space-3)",
                        borderRadius: "var(--radius-full)",
                        border: "1px solid",
                        borderColor: view === tab.id ? "var(--accent-soft)" : "transparent",
                        background: view === tab.id ? "var(--accent-soft)" : "transparent",
                        color: view === tab.id ? "var(--accent)" : "var(--text-muted)",
                        fontSize: "var(--text-xs)",
                        fontWeight: 500,
                        cursor: "pointer",
                        whiteSpace: "nowrap",
                        transition: "all var(--transition-fast)",
                    }}
                >
                    {tab.label}
                    {tab.count !== null && <span style={{ marginLeft: "var(--space-1)", opacity: 0.7 }}>{tab.count}</span>}
                </button>
            ))}
        </div>
    );
}

export function AgentChatPanelCurrentSurface() {
    const { view, setView, setChatBackView } = useAgentChatPanelRuntime();

    if (view === "threads") return <AgentChatPanelThreadsSurface />;
    if (view === "chat") return <AgentChatPanelChatSurface />;
    if (view === "trace") return <AgentChatPanelTraceSurface />;
    if (view === "usage") return <AgentChatPanelUsageSurface />;
    if (view === "context") return <AgentChatPanelContextSurface />;
    if (view === "coding-agents") return <AgentChatPanelCodingAgentsSurface />;
    if (view === "ai-training") return <AgentChatPanelAITrainingSurface />;
    if (view === "tasks") return <TasksView onOpenThreadView={() => { setChatBackView("tasks"); setView("chat"); }} />;
    if (view === "subagents") return <SubagentsView onOpenThreadView={() => { setChatBackView("subagents"); setView("chat"); }} onOpenTasksView={() => setView("tasks")} />;
    return <AgentChatPanelGraphSurface />;
}

export function AgentChatPanelThreadsSurface() {
    const { filteredThreads, searchQuery, setSearchQuery, setActiveThread, setView, setChatBackView, deleteThread } = useAgentChatPanelRuntime();

    return (
        <ThreadList
            threads={filteredThreads}
            searchQuery={searchQuery}
            onSearch={setSearchQuery}
            onSelect={(thread) => {
                setActiveThread(thread.id);
                setChatBackView("threads");
                setView("chat");
            }}
            onDelete={deleteThread}
        />
    );
}

export function AgentChatPanelChatSurface() {
    const runtime = useAgentChatPanelRuntime();
    return (
        <ChatView
            messages={runtime.messages}
            todos={runtime.todos}
            input={runtime.input}
            setInput={runtime.setInput}
            inputRef={runtime.inputRef}
            onKeyDown={runtime.handleKeyDown}
            agentSettings={runtime.agentSettings}
            isStreamingResponse={runtime.isStreamingResponse}
            activeThread={runtime.activeThread}
            messagesEndRef={runtime.messagesEndRef}
            onSendMessage={runtime.sendMessage}
            onStopStreaming={() => runtime.stopStreaming(runtime.activeThreadId)}
            onDeleteMessage={(messageId) => {
                const tid = runtime.activeThreadId;
                if (tid) runtime.deleteMessage(tid, messageId);
            }}
            onUpdateReasoningEffort={(v) => runtime.updateAgentSetting("reasoning_effort", v as AgentSettings["reasoning_effort"])}
            canStartGoalRun={runtime.canStartGoalRun}
            onStartGoalRun={runtime.startGoalRunFromPrompt}
        />
    );
}

export function AgentChatPanelTraceSurface() {
    const { scopedOperationalEvents, scopedCognitiveEvents, pendingApprovals, daemonTodosByThread, goalRunsForTrace } = useAgentChatPanelRuntime();
    return (
        <TraceView
            operationalEvents={scopedOperationalEvents}
            cognitiveEvents={scopedCognitiveEvents}
            pendingApprovals={pendingApprovals}
            todosByThread={daemonTodosByThread}
            goalRuns={goalRunsForTrace}
        />
    );
}

export function AgentChatPanelUsageSurface() {
    const { threads, allMessagesByThread } = useAgentChatPanelRuntime();
    return <UsageView threads={threads} messagesByThread={allMessagesByThread} />;
}

export function AgentChatPanelContextSurface() {
    const runtime = useAgentChatPanelRuntime();
    return (
        <ContextView
            agentSettings={runtime.agentSettings}
            snippets={runtime.snippets}
            transcripts={runtime.transcripts}
            scopePaneId={runtime.scopePaneId}
            threads={runtime.threads}
            activeThreadId={runtime.activeThreadId}
            latestContextSnapshot={runtime.latestContextSnapshot}
            memory={runtime.memory}
            updateMemory={runtime.updateMemory}
            historyQuery={runtime.historyQuery}
            setHistoryQuery={runtime.setHistoryQuery}
            historySummary={runtime.historySummary}
            historyHits={runtime.historyHits}
            symbolQuery={runtime.symbolQuery}
            setSymbolQuery={runtime.setSymbolQuery}
            symbolHits={runtime.symbolHits}
            scopeController={runtime.scopeController}
        />
    );
}

export function AgentChatPanelGraphSurface() {
    const { scopedOperationalEvents, approvals, scopePaneId } = useAgentChatPanelGraphData();

    return (
        <div style={{ padding: "var(--space-4)", height: "100%", overflow: "auto" }}>
            <MetricRibbon
                items={[
                    { label: "Commands", value: String(scopedOperationalEvents.filter((event) => event.kind === "command-started").length) },
                    { label: "Approvals", value: String(approvals.length) },
                    { label: "Scope", value: scopePaneId ?? "all panes" },
                ]}
            />
            <SectionTitle title="Execution Graph" subtitle="Visualized command pipeline" />
            <AgentExecutionGraph paneId={scopePaneId} />
        </div>
    );
}

export function AgentChatPanelCodingAgentsSurface() {
    return <CodingAgentsView />;
}

export function AgentChatPanelAITrainingSurface() {
    return <AITrainingView />;
}

function useAgentChatPanelGraphData() {
    const { scopedOperationalEvents, scopePaneId } = useAgentChatPanelRuntime();
    const approvals = useAgentMissionStore((s) => s.approvals);
    return { scopedOperationalEvents, approvals, scopePaneId };
}
