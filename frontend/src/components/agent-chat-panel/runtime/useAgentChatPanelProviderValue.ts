import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { abortThreadStream, useAgentStore } from "@/lib/agentStore";
import { getAgentBridge, shouldUseDaemonRuntime } from "@/lib/agentDaemonConfig";
import { fetchAgentRuns, isSubagentRun, type AgentRun } from "@/lib/agentRuns";
import { fetchThreadTodos } from "@/lib/agentTodos";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";
import { deriveSpawnedAgentTree } from "@/lib/spawnedAgentTree";
import type { SpawnedAgentTree } from "@/lib/spawnedAgentTree";
import { getTerminalController } from "@/lib/terminalRegistry";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useSnippetStore } from "@/lib/snippetStore";
import { useTranscriptStore } from "@/lib/transcriptStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentThread, AgentTodoItem } from "@/lib/agentStore";
import type { GoalRun } from "@/lib/goalRuns";
import type { Workspace } from "@/lib/types";
import type { WelesHealthState } from "@/lib/agentStore/types";
import { useDaemonAgentActions } from "./useDaemonAgentActions";
import { useDaemonAgentEvents } from "./useDaemonAgentEvents";
import {
  hydrateDaemonThreadIntoLocalState,
  reloadDaemonThreadIntoLocalState,
} from "./daemonHelpers";
import { useLegacyAgentMessaging } from "./useLegacyAgentMessaging";
import type { AgentChatPanelRuntimeValue, AgentChatPanelView } from "./types";
import {
  pinnedMessageBudgetChars,
  sumMessageContentChars,
} from "@/lib/agent-client/pinnedMessageBudget";

const EMPTY_MESSAGES: ReturnType<typeof useAgentStore.getState>["messages"][string] = [];

type SpawnedAgentNavigationState = {
  tree: SpawnedAgentTree<AgentRun> | null;
  canGoBackThread: boolean;
  threadNavigationDepth: number;
  backThreadTitle: string | null;
};

type RemoteAgentThread = {
  id: string;
  title: string;
  messages: unknown[];
};

type PendingSpawnedThreadHydration = {
  promise: Promise<string | null>;
};

const pendingSpawnedThreadHydrations = new Map<string, PendingSpawnedThreadHydration>();

export function resetPendingSpawnedThreadHydrationsForTest(): void {
  pendingSpawnedThreadHydrations.clear();
}

function findLocalThreadByDaemonThreadId(
  threads: AgentThread[],
  daemonThreadId: string,
): AgentThread | undefined {
  return threads.find((thread) => thread.daemonThreadId === daemonThreadId);
}

export function deriveSpawnedAgentNavigationState({
  activeThread,
  threads,
  threadHistoryStack,
  runs,
}: {
  activeThread: AgentThread | undefined;
  threads: AgentThread[];
  threadHistoryStack: string[];
  runs: AgentRun[];
}): SpawnedAgentNavigationState {
  const activeDaemonThreadId = activeThread?.daemonThreadId ?? null;
  const backThreadId = threadHistoryStack[threadHistoryStack.length - 1] ?? null;
  const backThreadTitle = backThreadId
    ? threads.find((thread) => thread.id === backThreadId)?.title ?? null
    : null;

  return {
    tree: deriveSpawnedAgentTree(runs, activeDaemonThreadId),
    canGoBackThread: threadHistoryStack.length > 0,
    threadNavigationDepth: threadHistoryStack.length,
    backThreadTitle,
  };
}

export async function openSpawnedAgentThreadFromRun({
  activeThreadId,
  threads,
  workspaces,
  run,
  messageLimit,
  getRemoteThread,
  fetchThreadTodos: fetchThreadTodosForThread,
  createThread,
  addMessage,
  setThreadDaemonId,
  setThreadTodos,
  openSpawnedThread,
}: {
  activeThreadId: string | null;
  threads: AgentThread[];
  workspaces: Workspace[];
  run: AgentRun;
  messageLimit: number | null;
  getRemoteThread?: (
    threadId: string,
    options: { messageLimit: number | null },
  ) => Promise<{ id: string; title: string; messages: unknown[] } | null>;
  fetchThreadTodos: (threadId: string) => Promise<AgentTodoItem[]>;
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  openSpawnedThread: ReturnType<typeof useAgentStore.getState>["openSpawnedThread"];
}): Promise<boolean> {
  if (!activeThreadId || !run.thread_id) {
    return false;
  }

  const completeSpawnedThreadOpen = async (
    pendingHydration: PendingSpawnedThreadHydration,
  ) => {
    const localThreadId = await pendingHydration.promise;
    if (!localThreadId) {
      return false;
    }
    const currentActiveThreadId = useAgentStore.getState().activeThreadId;
    if (currentActiveThreadId === localThreadId) {
      return true;
    }
    if (currentActiveThreadId !== activeThreadId) {
      return false;
    }
    openSpawnedThread(activeThreadId, localThreadId);
    return true;
  };

  const pendingHydration = pendingSpawnedThreadHydrations.get(run.thread_id);
  if (pendingHydration) {
    return completeSpawnedThreadOpen(pendingHydration);
  }

  const existingThread = findLocalThreadByDaemonThreadId(threads, run.thread_id);
  if (existingThread) {
    if (existingThread.id === activeThreadId) {
      return false;
    }
    openSpawnedThread(activeThreadId, existingThread.id);
    return true;
  }

  if (!getRemoteThread) {
    return false;
  }

  let resolveHydration!: (threadId: string | null) => void;
  let rejectHydration!: (reason?: unknown) => void;
  const hydratePromise = new Promise<string | null>((resolve, reject) => {
    resolveHydration = resolve;
    rejectHydration = reject;
  });

  const pendingEntry: PendingSpawnedThreadHydration = { promise: hydratePromise };
  pendingSpawnedThreadHydrations.set(run.thread_id, pendingEntry);

  void (async () => {
    try {
      const remoteThread = await getRemoteThread(run.thread_id!, { messageLimit });
      if (!remoteThread) {
        resolveHydration(null);
        return;
      }

      const preservedSelection = {
        activeThreadId: useAgentStore.getState().activeThreadId,
        threadHistoryStack: [...useAgentStore.getState().threadHistoryStack],
      };
      const localThreadId = await hydrateDaemonThreadIntoLocalState({
        sessionId: run.session_id,
        fallbackTitle: run.title,
        workspaces,
        remoteThread: remoteThread as any,
        fetchThreadTodos: fetchThreadTodosForThread,
        createThread,
        addMessage,
        setThreadDaemonId,
        setThreadTodos,
        onThreadReady: () => {
          useAgentStore.setState({
            activeThreadId: preservedSelection.activeThreadId,
            threadHistoryStack: preservedSelection.threadHistoryStack,
          });
        },
      });
      resolveHydration(localThreadId);
    } catch (error) {
      rejectHydration(error);
    } finally {
      if (pendingSpawnedThreadHydrations.get(run.thread_id!) === pendingEntry) {
        pendingSpawnedThreadHydrations.delete(run.thread_id!);
      }
    }
  })();

  try {
    return completeSpawnedThreadOpen(pendingEntry);
  } finally {
    // Cleanup happens in the async hydration runner once the promise settles.
  }
}

export function useAgentChatPanelProviderValue(): {
  isOpen: boolean;
  value: AgentChatPanelRuntimeValue;
} {
  const isOpen = useWorkspaceStore((state) => state.agentPanelOpen);
  const togglePanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  const activePaneId = useWorkspaceStore((state) => state.activePaneId());
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const workspaces = useWorkspaceStore((state) => state.workspaces);

  const threads = useAgentStore((state) => state.threads);
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const threadHistoryStack = useAgentStore((state) => state.threadHistoryStack);
  const createThread = useAgentStore((state) => state.createThread);
  const deleteThread = useAgentStore((state) => state.deleteThread);
  const setActiveThread = useAgentStore((state) => state.setActiveThread);
  const openSpawnedThreadInStore = useAgentStore((state) => state.openSpawnedThread);
  const goBackThread = useAgentStore((state) => state.goBackThread);
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
  const [spawnedAgentRuns, setSpawnedAgentRuns] = useState<AgentRun[]>([]);
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

  const refreshSpawnedAgentRuns = useCallback(async () => {
    const runs = await fetchAgentRuns();
    setSpawnedAgentRuns(runs.filter(isSubagentRun));
  }, []);

  useEffect(() => {
    void refreshSpawnedAgentRuns();
    const interval = window.setInterval(() => {
      void refreshSpawnedAgentRuns();
    }, 5000);
    return () => window.clearInterval(interval);
  }, [refreshSpawnedAgentRuns]);

  useEffect(() => {
    const amux = getAgentBridge();
    if (!amux?.onAgentEvent) {
      return;
    }

    const unsubscribe = amux.onAgentEvent((event: any) => {
      if (event?.type === "task_update") {
        void refreshSpawnedAgentRuns();
      }
    });

    return () => unsubscribe?.();
  }, [refreshSpawnedAgentRuns]);

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
  const spawnedAgentNavigation = useMemo(
    () => deriveSpawnedAgentNavigationState({
      activeThread,
      threads,
      threadHistoryStack,
      runs: spawnedAgentRuns,
    }),
    [activeThread, spawnedAgentRuns, threadHistoryStack, threads],
  );

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
  const canOpenSpawnedThread = useCallback((run: AgentRun) => {
    if (!run.thread_id) {
      return false;
    }

    const existingThread = findLocalThreadByDaemonThreadId(threads, run.thread_id);
    if (existingThread) {
      return existingThread.id !== activeThreadId;
    }

    return Boolean(getAgentBridge()?.agentGetThread);
  }, [activeThreadId, threads]);
  const openSpawnedThread = useCallback(async (run: AgentRun) => {
    const amux = getAgentBridge();
    return openSpawnedAgentThreadFromRun({
      activeThreadId,
      threads,
      workspaces,
      run,
      messageLimit: resolveReactChatHistoryMessageLimit(agentSettings.react_chat_history_page_size) ?? null,
      getRemoteThread: amux?.agentGetThread
        ? async (threadId, options): Promise<RemoteAgentThread | null> => {
          const result = await amux.agentGetThread?.(threadId, options);
          return (result as RemoteAgentThread | null | undefined) ?? null;
        }
        : undefined,
      fetchThreadTodos,
      createThread,
      addMessage,
      setThreadDaemonId,
      setThreadTodos,
      openSpawnedThread: openSpawnedThreadInStore,
    });
  }, [
    activeThreadId,
    addMessage,
    agentSettings.react_chat_history_page_size,
    createThread,
    openSpawnedThreadInStore,
    setThreadDaemonId,
    setThreadTodos,
    threads,
    workspaces,
  ]);

  const sendMessage = useCallback((payload: { text: string; contentBlocksJson?: string | null; localContentBlocks?: import("@/lib/agentStore/types").AgentContentBlock[] }) => {
    if (!payload.text) return;
    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && sendDaemonMessage(payload)) {
      return;
    }
    sendMessageLegacy(payload.text);
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

  const pinMessageForCompaction = useCallback(async (threadId: string, messageId: string) => {
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
    const daemonThreadId = thread?.daemonThreadId ?? (threadId === activeThreadId ? daemonThreadIdRef.current : null);
    const amux = getAgentBridge();

    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && daemonThreadId && amux?.agentPinThreadMessageForCompaction) {
      const result = await amux.agentPinThreadMessageForCompaction(daemonThreadId, messageId) as AmuxThreadMessagePinResult;
      if (result?.ok) {
        await reloadDaemonThreadIntoLocalState({
          daemonThreadId,
          setThreadTodos,
          setDaemonTodosByThread,
        });
      }
      return result;
    }

    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        [threadId]: (state.messages[threadId] ?? []).map((message) =>
          message.id === messageId ? { ...message, pinnedForCompaction: true } : message),
      },
    }));
    return {
      ok: true,
      thread_id: threadId,
      message_id: messageId,
      current_pinned_chars: 0,
      pinned_budget_chars: 0,
    } satisfies AmuxThreadMessagePinResult;
  }, [activeThreadId, agentSettings.agent_backend, setThreadTodos]);

  const unpinMessageForCompaction = useCallback(async (threadId: string, messageId: string) => {
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
    const daemonThreadId = thread?.daemonThreadId ?? (threadId === activeThreadId ? daemonThreadIdRef.current : null);
    const amux = getAgentBridge();

    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && daemonThreadId && amux?.agentUnpinThreadMessageForCompaction) {
      const result = await amux.agentUnpinThreadMessageForCompaction(daemonThreadId, messageId) as AmuxThreadMessagePinResult;
      if (result?.ok) {
        await reloadDaemonThreadIntoLocalState({
          daemonThreadId,
          setThreadTodos,
          setDaemonTodosByThread,
        });
      }
      return result;
    }

    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        [threadId]: (state.messages[threadId] ?? []).map((message) =>
          message.id === messageId ? { ...message, pinnedForCompaction: false } : message),
      },
    }));
    return {
      ok: true,
      thread_id: threadId,
      message_id: messageId,
      current_pinned_chars: 0,
      pinned_budget_chars: 0,
    } satisfies AmuxThreadMessagePinResult;
  }, [activeThreadId, agentSettings.agent_backend, setThreadTodos]);

  const handleSend = useCallback(() => {
    const text = input.trim();
    if (!text) return;
    sendMessage({ text });
    setInput("");
  }, [input, sendMessage]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  const pinnedMessages = useMemo(
    () => messages.filter((message) => message.pinnedForCompaction),
    [messages],
  );
  const pinnedUsageChars = useMemo(
    () => sumMessageContentChars(pinnedMessages),
    [pinnedMessages],
  );
  const pinnedBudgetChars = useMemo(() => {
    const activeProviderId = (agentSettings as { active_provider?: keyof typeof agentSettings }).active_provider;
    const activeProvider = activeProviderId
      ? agentSettings[activeProviderId] as { context_window_tokens?: number | null } | undefined
      : undefined;
    const contextWindowTokens = Number(activeProvider?.context_window_tokens ?? agentSettings.context_window_tokens ?? 0);
    return pinnedMessageBudgetChars(contextWindowTokens);
  }, [agentSettings]);
  const pinnedOverBudget = pinnedUsageChars > pinnedBudgetChars;

  const tabItems = [
    { id: "threads", label: "Threads", count: threads.length },
    { id: "chat", label: "Chat", count: null },
    ...(pinnedMessages.length > 0 ? [{ id: "pinned" as const, label: "Pinned", count: pinnedMessages.length }] : []),
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
    spawnedAgentTree: spawnedAgentNavigation.tree,
    canGoBackThread: spawnedAgentNavigation.canGoBackThread,
    goBackThread,
    canOpenSpawnedThread,
    openSpawnedThread,
    threadNavigationDepth: spawnedAgentNavigation.threadNavigationDepth,
    backThreadTitle: spawnedAgentNavigation.backThreadTitle,
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
    pinMessageForCompaction,
    unpinMessageForCompaction,
    stopStreaming,
    handleSend,
    handleKeyDown,
    builtinAgentSetup,
    canStartGoalRun,
    cancelBuiltinAgentSetup,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
    tabItems,
    pinnedMessages,
    pinnedBudgetChars,
    pinnedUsageChars,
    pinnedOverBudget,
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
    goBackThread,
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
    spawnedAgentNavigation,
    pinMessageForCompaction,
    pinnedBudgetChars,
    pinnedMessages,
    pinnedOverBudget,
    pinnedUsageChars,
    scopeController,
    scopePaneId,
    scopedCognitiveEvents,
    scopedOperationalEvents,
    searchQuery,
    canOpenSpawnedThread,
    openSpawnedThread,
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
    unpinMessageForCompaction,
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
