import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { abortThreadStream, useAgentStore } from "@/lib/agentStore";
import { getAgentBridge, shouldUseDaemonRuntime } from "@/lib/agentDaemonConfig";
import { getTerminalController } from "@/lib/terminalRegistry";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useSnippetStore } from "@/lib/snippetStore";
import { useTranscriptStore } from "@/lib/transcriptStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentTodoItem } from "@/lib/agentStore";
import type { GoalRun } from "@/lib/goalRuns";
import type { WelesHealthState } from "@/lib/agentStore/types";
import { useDaemonAgentActions } from "./useDaemonAgentActions";
import { useDaemonAgentEvents } from "./useDaemonAgentEvents";
import { useLegacyAgentMessaging } from "./useLegacyAgentMessaging";
import type { AgentChatPanelRuntimeValue, AgentChatPanelView } from "./types";

const EMPTY_MESSAGES: ReturnType<typeof useAgentStore.getState>["messages"][string] = [];

export function useAgentChatPanelProviderValue(): {
  isOpen: boolean;
  value: AgentChatPanelRuntimeValue;
} {
  const isOpen = useWorkspaceStore((state) => state.agentPanelOpen);
  const togglePanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  const activePaneId = useWorkspaceStore((state) => state.activePaneId());
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());

  const threads = useAgentStore((state) => state.threads);
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const createThread = useAgentStore((state) => state.createThread);
  const deleteThread = useAgentStore((state) => state.deleteThread);
  const setActiveThread = useAgentStore((state) => state.setActiveThread);
  const addMessage = useAgentStore((state) => state.addMessage);
  const deleteMessage = useAgentStore((state) => state.deleteMessage);
  const updateLastAssistantMessage = useAgentStore((state) => state.updateLastAssistantMessage);
  const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
  const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const searchQuery = useAgentStore((state) => state.searchQuery);
  const setSearchQuery = useAgentStore((state) => state.setSearchQuery);
  const storeMessages = useAgentStore((state) => activeThreadId ? state.messages[activeThreadId] : undefined);
  const storeTodos = useAgentStore((state) => activeThreadId ? state.todos[activeThreadId] : undefined);
  const allMessagesByThread = useAgentStore((state) => state.messages);
  const activeThread = threads.find((thread) => thread.id === activeThreadId);

  const operationalEvents = useAgentMissionStore((state) => state.operationalEvents);
  const cognitiveEvents = useAgentMissionStore((state) => state.cognitiveEvents);
  const contextSnapshots = useAgentMissionStore((state) => state.contextSnapshots);
  const approvals = useAgentMissionStore((state) => state.approvals);
  const memory = useAgentMissionStore((state) => state.memory);
  const updateMemory = useAgentMissionStore((state) => state.updateMemory);
  const historySummary = useAgentMissionStore((state) => state.historySummary);
  const historyHits = useAgentMissionStore((state) => state.historyHits);
  const symbolHits = useAgentMissionStore((state) => state.symbolHits);
  const snippets = useSnippetStore((state) => state.snippets);
  const transcripts = useTranscriptStore((state) => state.transcripts);
  const addNotification = useNotificationStore((state) => state.addNotification);

  const [input, setInput] = useState("");
  const [view, setView] = useState<AgentChatPanelView>("threads");
  const [chatBackView, setChatBackView] = useState<AgentChatPanelView>("threads");
  const [historyQuery, setHistoryQuery] = useState("");
  const [symbolQuery, setSymbolQuery] = useState("");
  const [daemonTodosByThread, setDaemonTodosByThread] = useState<Record<string, AgentTodoItem[]>>({});
  const [goalRunsForTrace, setGoalRunsForTrace] = useState<GoalRun[]>([]);
  const [latestDivergentSessionId, setLatestDivergentSessionId] = useState<string | null>(null);
  const [welesHealth, setWelesHealth] = useState<WelesHealthState | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const daemonThreadIdRef = useRef<string | null>(null);
  const daemonLocalThreadRef = useRef<string | null>(null);
  const pendingGatewayMessagesRef = useRef<Array<{ role: "user"; content: string; inputTokens: number; outputTokens: number; totalTokens: number; isCompactionSummary: boolean }>>([]);
  const goalRunWorkspacesRef = useRef<Record<string, string>>({});

  useDaemonAgentEvents({
    agentBackend: agentSettings.agent_backend,
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
  });

  const stopStreaming = useCallback((threadId?: string | null) => {
    const targetThreadId = threadId ?? activeThreadId;
    if (!targetThreadId) return;

    if (shouldUseDaemonRuntime(agentSettings.agent_backend)) {
      const amux = getAgentBridge();
      const daemonThreadId = daemonThreadIdRef.current;
      if (daemonThreadId && amux?.agentStopStream) {
        void amux.agentStopStream(daemonThreadId);
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
  }, [activeThreadId, agentSettings.agent_backend, updateLastAssistantMessage]);

  const { sendMessageLegacy } = useLegacyAgentMessaging({
    activeThreadId,
    agentSettings,
    addMessage,
    abortRef,
    createThread,
    setView,
    stopStreaming,
    updateLastAssistantMessage,
  });

  const {
    builtinAgentSetup,
    cancelBuiltinAgentSetup,
    canStartGoalRun,
    sendDaemonMessage,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
  } = useDaemonAgentActions({
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    addNotification,
    agentSettings,
    createThread,
    daemonThreadIdRef,
    daemonLocalThreadRef,
    goalRunWorkspacesRef,
    goalRunsForTrace,
    latestDivergentSessionId,
    setActiveThread,
    setLatestDivergentSessionId,
    setView,
  });

  useEffect(() => {
    if (threads.length === 0) return;
    threads.forEach((thread) => {
      if (!thread.daemonThreadId) return;
      const items = daemonTodosByThread[thread.daemonThreadId];
      if (!items) return;
      setThreadTodos(thread.id, items);
    });
  }, [daemonTodosByThread, setThreadTodos, threads]);

  const messages = useMemo(() => storeMessages ?? EMPTY_MESSAGES, [storeMessages]);
  const todos = useMemo(() => storeTodos ?? [], [storeTodos]);
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
    if (isOpen && activeThreadId) {
      setView("chat");
      setTimeout(() => inputRef.current?.focus(), 100);
    } else if (isOpen) {
      setView("threads");
    }
  }, [activeThreadId, isOpen]);

  const filteredThreads = searchQuery
    ? threads.filter(
      (thread) => thread.title.toLowerCase().includes(searchQuery.toLowerCase())
        || thread.lastMessagePreview.toLowerCase().includes(searchQuery.toLowerCase()),
    )
    : threads;
  const isStreamingResponse = messages.some((message) => message.role === "assistant" && message.isStreaming);

  const sendMessage = useCallback((text: string) => {
    if (!text) return;
    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && sendDaemonMessage(text)) {
      return;
    }
    sendMessageLegacy(text);
  }, [agentSettings.agent_backend, sendDaemonMessage, sendMessageLegacy]);

  const sendParticipantSuggestion = useCallback(async (threadId: string, suggestionId: string, forceSend = false) => {
    const amux = getAgentBridge();
    if (!amux?.agentSendParticipantSuggestion) {
      return;
    }
    if (forceSend && activeThreadId) {
      stopStreaming(activeThreadId);
    }
    await amux.agentSendParticipantSuggestion({ threadId, suggestionId, sessionId: null });
  }, [activeThreadId, stopStreaming]);

  const dismissParticipantSuggestion = useCallback(async (threadId: string, suggestionId: string) => {
    const amux = getAgentBridge();
    if (!amux?.agentDismissParticipantSuggestion) {
      return;
    }
    await amux.agentDismissParticipantSuggestion({ threadId, suggestionId, sessionId: null });
  }, []);

  const handleSend = useCallback(() => {
    const text = input.trim();
    if (!text) return;
    sendMessage(text);
    setInput("");
  }, [input, sendMessage]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
  }, [handleSend]);

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
    sendParticipantSuggestion,
    dismissParticipantSuggestion,
    deleteMessage,
    stopStreaming,
    handleSend,
    handleKeyDown,
    builtinAgentSetup,
    canStartGoalRun,
    cancelBuiltinAgentSetup,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
    tabItems,
    welesHealth,
  }), [
    activeThread,
    activeThreadId,
    activeWorkspace,
    agentSettings,
    updateAgentSetting,
    allMessagesByThread,
    chatBackView,
    builtinAgentSetup,
    canStartGoalRun,
    cancelBuiltinAgentSetup,
    startGoalRunFromPrompt,
    daemonTodosByThread,
    deleteMessage,
    deleteThread,
    dismissParticipantSuggestion,
    filteredThreads,
    goalRunsForTrace,
    handleSend,
    handleKeyDown,
    historyHits,
    historyQuery,
    historySummary,
    input,
    isStreamingResponse,
    latestContextSnapshot,
    memory,
    messages,
    pendingApprovals,
    scopeController,
    scopePaneId,
    scopedCognitiveEvents,
    scopedOperationalEvents,
    searchQuery,
    setActiveThread,
    setSearchQuery,
    snippets,
    stopStreaming,
    submitBuiltinAgentSetup,
    symbolHits,
    symbolQuery,
    tabItems,
    threads,
    todos,
    togglePanel,
    transcripts,
    updateMemory,
    usageMessageCount,
    view,
    welesHealth,
    createThread,
    sendMessage,
    sendParticipantSuggestion,
  ]);

  return { isOpen, value };
}
