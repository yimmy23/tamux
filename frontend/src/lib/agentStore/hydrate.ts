import {
  AGENT_ACTIVE_THREAD_FILE,
  AGENT_CHAT_FILE,
  AGENT_DAEMON_THREAD_MAP_FILE,
  buildHydratedRemoteThread,
  type AgentChatState,
  deserializeMessage,
  deserializeThread,
  getAgentDbApi,
  readPersistedJson,
  serializeMessage,
  serializeThread,
  shouldPersistHistory,
  syncChatCounters,
} from "./history";
import { getBridge } from "../bridge";
import {
  DEFAULT_AGENT_SETTINGS,
  type DiskAgentSettings,
  looksLikeDaemonAgentConfig,
  normalizeAgentSettingsFromSource,
} from "./settings";
import { useAgentStore } from "./store";

export async function hydrateAgentStore(): Promise<void> {
  const bridge = getBridge();
  let configuredBackend = DEFAULT_AGENT_SETTINGS.agent_backend;
  let agentSettingsHydrated = false;

  if (bridge?.agentGetConfig) {
    const daemonState = await bridge.agentGetConfig().catch(() => null);
    if (looksLikeDaemonAgentConfig(daemonState)) {
      const merged = normalizeAgentSettingsFromSource(daemonState as DiskAgentSettings);
      configuredBackend = merged.agent_backend;
      useAgentStore.setState({
        agentSettings: merged,
        agentSettingsDirty: false,
      });
      agentSettingsHydrated = true;
    }
  } else {
    agentSettingsHydrated = true;
  }

  useAgentStore.setState({ agentSettingsHydrated });

  if (!shouldPersistHistory(configuredBackend)) {
    const amux = getBridge();
    if (amux?.agentListThreads) {
      const remoteThreads = await amux.agentListThreads().catch(() => []);
      if (Array.isArray(remoteThreads) && remoteThreads.length > 0) {
        const messages: AgentChatState["messages"] = {};
        const threads = [];
        for (const remoteThread of remoteThreads) {
          const hydrated = buildHydratedRemoteThread(
            remoteThread ?? {},
            useAgentStore.getState().agentSettings.agent_name,
          );
          if (!hydrated) {
            continue;
          }
          threads.push(hydrated.thread);
          messages[hydrated.thread.id] = hydrated.messages;
        }
        if (threads.length > 0) {
          const sortedThreads = threads.sort((left, right) => right.updatedAt - left.updatedAt);
          const hydrated: AgentChatState = {
            threads: sortedThreads,
            messages,
            todos: {},
            activeThreadId: sortedThreads[0]?.id ?? null,
          };
          syncChatCounters(hydrated);
          useAgentStore.setState(hydrated);
        }
      }
    }
    return;
  }

  const api = getAgentDbApi();
  const daemonThreadMap = await readPersistedJson<Record<string, string>>(AGENT_DAEMON_THREAD_MAP_FILE) ?? {};
  const savedActiveThread = await readPersistedJson<{ activeThreadId: string | null }>(AGENT_ACTIVE_THREAD_FILE);
  const dbThreads = await api?.dbListThreads?.();
  if (Array.isArray(dbThreads) && dbThreads.length > 0) {
    const messages: AgentChatState["messages"] = {};
    for (const thread of dbThreads) {
      const threadMessages = await api?.dbListMessages?.(thread.id, 500) ?? [];
      messages[thread.id] = threadMessages.map(deserializeMessage);
    }

    const hydratedThreads = dbThreads.map((thread) => ({
      ...deserializeThread(thread),
      daemonThreadId: daemonThreadMap[thread.id] ?? null,
      messageCount: messages[thread.id]?.length ?? thread.message_count,
      lastMessagePreview: messages[thread.id]?.[messages[thread.id].length - 1]?.content?.slice(0, 100) ?? thread.last_preview ?? "",
    }));
    const savedId = savedActiveThread?.activeThreadId;
    const restoredId = (savedId && hydratedThreads.some((thread) => thread.id === savedId))
      ? savedId
      : (hydratedThreads.length > 0
        ? hydratedThreads.reduce((left, right) => (left.updatedAt >= right.updatedAt ? left : right)).id
        : null);
    const hydrated: AgentChatState = {
      threads: hydratedThreads,
      messages,
      todos: {},
      activeThreadId: restoredId,
    };
    syncChatCounters(hydrated);
    useAgentStore.setState(hydrated);
    return;
  }

  const legacyChat = await readPersistedJson<AgentChatState>(AGENT_CHAT_FILE);
  if (!legacyChat || !Array.isArray(legacyChat.threads) || typeof legacyChat.messages !== "object") {
    return;
  }

  const hydrated: AgentChatState = {
    threads: legacyChat.threads.map((thread) => ({
      ...thread,
      daemonThreadId: daemonThreadMap[thread.id] ?? thread.daemonThreadId ?? null,
    })),
    messages: legacyChat.messages,
    todos: {},
    activeThreadId: legacyChat.activeThreadId ?? null,
  };
  syncChatCounters(hydrated);
  useAgentStore.setState(hydrated);

  for (const thread of hydrated.threads) {
    await api?.dbCreateThread?.(serializeThread(thread));
    for (const message of hydrated.messages[thread.id] ?? []) {
      await api?.dbAddMessage?.(serializeMessage(message));
    }
  }
}
