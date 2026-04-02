import { scheduleJsonWrite } from "../persistence";
import {
  AGENT_ACTIVE_THREAD_FILE,
  type AgentChatState,
  getAgentDbApi,
  nextMessageId,
  nextThreadId,
  persistDaemonThreadMap,
  serializeMessage,
  serializeThread,
  shouldPersistHistory,
} from "./history";
import { normalizeAgentProviderId } from "./providers";
import type { AgentSettings } from "./settings";
import type { AgentState, AgentStoreGet, AgentStoreSet } from "./storeTypes";
import type { AgentMessage } from "./types";

type ThreadActionKeys =
  | "createThread"
  | "deleteThread"
  | "setActiveThread"
  | "searchThreads"
  | "addMessage"
  | "updateLastAssistantMessage"
  | "getThreadMessages"
  | "deleteMessage"
  | "setThreadTodos"
  | "getThreadTodos"
  | "setThreadDaemonId"
  | "toggleAgentPanel"
  | "setSearchQuery"
  | "getThreadsForPane";

function shouldPersistCurrentHistory(agentSettings: AgentSettings): boolean {
  return shouldPersistHistory(agentSettings.agent_backend);
}

export function createThreadActions(
  set: AgentStoreSet,
  get: AgentStoreGet,
): Pick<AgentState, ThreadActionKeys> {
  return {
    createThread: (opts) => {
      const id = nextThreadId();
      const now = Date.now();
      const thread = {
        id,
        daemonThreadId: null,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        paneId: opts.paneId ?? null,
        agent_name: get().agentSettings.agent_name,
        title: opts.title ?? "New Conversation",
        createdAt: now,
        updatedAt: now,
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
      set((state) => {
        const next: AgentChatState = {
          threads: [thread, ...state.threads],
          messages: { ...state.messages, [id]: [] },
          todos: { ...state.todos, [id]: [] },
          activeThreadId: id,
        };
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(next.threads);
          scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
          void getAgentDbApi()?.dbCreateThread?.(serializeThread(thread));
        }
        return next;
      });
      return id;
    },
    deleteThread: (id) => {
      set((state) => {
        const { [id]: _message, ...remainingMessages } = state.messages;
        const { [id]: _todo, ...remainingTodos } = state.todos;
        const next: AgentChatState = {
          threads: state.threads.filter((thread) => thread.id !== id),
          messages: remainingMessages,
          todos: remainingTodos,
          activeThreadId: state.activeThreadId === id ? null : state.activeThreadId,
        };
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(next.threads);
          void getAgentDbApi()?.dbDeleteThread?.(id);
        }
        return next;
      });
    },
    setActiveThread: (id) => {
      set({ activeThreadId: id });
      if (shouldPersistCurrentHistory(get().agentSettings)) {
        scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: id });
      }
    },
    searchThreads: (query) => {
      const lower = query.toLowerCase();
      return get().threads.filter((thread) =>
        thread.title.toLowerCase().includes(lower)
        || thread.lastMessagePreview.toLowerCase().includes(lower)
        || thread.agent_name.toLowerCase().includes(lower));
    },
    addMessage: (threadId, message) => {
      const fullMessage: AgentMessage = {
        ...message,
        id: nextMessageId(),
        threadId,
        createdAt: Date.now(),
      };
      set((state) => {
        const threadMessages = [...(state.messages[threadId] ?? []), fullMessage];
        const next: AgentChatState = {
          messages: { ...state.messages, [threadId]: threadMessages },
          todos: state.todos,
          threads: state.threads.map((thread) =>
            thread.id === threadId
              ? {
                ...thread,
                messageCount: thread.messageCount + 1,
                updatedAt: Date.now(),
                totalInputTokens: thread.totalInputTokens + message.inputTokens,
                totalOutputTokens: thread.totalOutputTokens + message.outputTokens,
                totalTokens: thread.totalTokens + message.totalTokens,
                lastMessagePreview: message.content.slice(0, 100),
              }
              : thread),
          activeThreadId: state.activeThreadId,
        };
        const updatedThread = next.threads.find((thread) => thread.id === threadId);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          void (async () => {
            const api = getAgentDbApi();
            if (updatedThread) {
              await api?.dbCreateThread?.(serializeThread(updatedThread));
            }
            await api?.dbAddMessage?.(serializeMessage(fullMessage));
          })();
        }
        return next;
      });
    },
    updateLastAssistantMessage: (threadId, content, streaming, meta) => {
      set((state) => {
        const messages = state.messages[threadId];
        if (!messages || messages.length === 0) {
          return state;
        }
        const lastMessage = messages[messages.length - 1];
        if (lastMessage.role !== "assistant") {
          return state;
        }
        const nextInputTokens = meta?.inputTokens ?? lastMessage.inputTokens;
        const nextOutputTokens = meta?.outputTokens ?? lastMessage.outputTokens;
        const nextTotalTokens = meta?.totalTokens ?? lastMessage.totalTokens;
        const updatedLastMessage: AgentMessage = {
          ...lastMessage,
          content,
          isStreaming: streaming ?? false,
          inputTokens: nextInputTokens,
          outputTokens: nextOutputTokens,
          totalTokens: nextTotalTokens,
          reasoning: meta?.reasoning ?? lastMessage.reasoning,
          reasoningTokens: meta?.reasoningTokens ?? lastMessage.reasoningTokens,
          audioTokens: meta?.audioTokens ?? lastMessage.audioTokens,
          videoTokens: meta?.videoTokens ?? lastMessage.videoTokens,
          cost: meta?.cost ?? lastMessage.cost,
          tps: meta?.tps ?? lastMessage.tps,
          toolCalls: meta?.toolCalls ?? lastMessage.toolCalls,
          provider: meta?.provider ?? lastMessage.provider,
          model: meta?.model ?? lastMessage.model,
          api_transport: meta?.api_transport ?? lastMessage.api_transport,
          responseId: meta?.responseId ?? lastMessage.responseId,
        };
        const updatedMessages = [...messages.slice(0, -1), updatedLastMessage];
        const tokenDeltaIn = nextInputTokens - lastMessage.inputTokens;
        const tokenDeltaOut = nextOutputTokens - lastMessage.outputTokens;
        const tokenDeltaTotal = nextTotalTokens - lastMessage.totalTokens;
        const nextThreads = state.threads.map((thread) =>
          thread.id === threadId
            ? {
              ...thread,
              totalInputTokens: thread.totalInputTokens + tokenDeltaIn,
              totalOutputTokens: thread.totalOutputTokens + tokenDeltaOut,
              totalTokens: thread.totalTokens + tokenDeltaTotal,
              updatedAt: Date.now(),
              lastMessagePreview: content.slice(0, 100),
            }
            : thread);
        const updatedThread = nextThreads.find((thread) => thread.id === threadId);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          void (async () => {
            const api = getAgentDbApi();
            if (updatedThread) {
              await api?.dbCreateThread?.(serializeThread(updatedThread));
            }
            await api?.dbAddMessage?.(serializeMessage(updatedLastMessage));
          })();
        }
        return { messages: { ...state.messages, [threadId]: updatedMessages }, threads: nextThreads };
      });
    },
    getThreadMessages: (threadId) => get().messages[threadId] ?? [],
    deleteMessage: (threadId, messageId) => {
      set((state) => {
        const messages = state.messages[threadId];
        if (!messages) {
          return state;
        }
        const filtered = messages.filter((message) => message.id !== messageId);
        if (filtered.length === messages.length) {
          return state;
        }
        return {
          messages: { ...state.messages, [threadId]: filtered },
          threads: state.threads.map((thread) =>
            thread.id === threadId
              ? { ...thread, messageCount: Math.max(0, thread.messageCount - 1), updatedAt: Date.now() }
              : thread),
        };
      });
      void getAgentDbApi()?.dbDeleteMessage?.(threadId, messageId);
    },
    setThreadTodos: (threadId, todos) => {
      set((state) => ({
        todos: { ...state.todos, [threadId]: [...todos].sort((left, right) => left.position - right.position) },
      }));
    },
    getThreadTodos: (threadId) => get().todos[threadId] ?? [],
    setThreadDaemonId: (threadId, daemonThreadId) => {
      set((state) => {
        const threads = state.threads.map((thread) =>
          thread.id === threadId ? { ...thread, daemonThreadId } : thread);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(threads);
        }
        return { threads };
      });
    },
    toggleAgentPanel: () => set((state) => ({ agentPanelOpen: !state.agentPanelOpen })),
    setSearchQuery: (query) => set({ searchQuery: query }),
    getThreadsForPane: (paneId) => get().threads.filter((thread) => thread.paneId === paneId),
  };
}

export { normalizeAgentProviderId };
