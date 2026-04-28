import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { useAgentStore } from "@/lib/agentStore";

export async function openThreadTarget(runtime: AgentChatPanelRuntimeValue, targetThreadId: string): Promise<boolean> {
  const target = targetThreadId.trim();
  if (!target) return false;

  const local = findThread(runtime.threads, target) ?? findThread(useAgentStore.getState().threads, target);
  if (local) {
    runtime.openThread(local.id);
    return true;
  }

  await runtime.refreshThreadList();
  const refreshed = findThread(useAgentStore.getState().threads, target);
  if (!refreshed) return false;
  runtime.openThread(refreshed.id);
  return true;
}

function findThread(threads: AgentChatPanelRuntimeValue["threads"], targetThreadId: string) {
  return threads.find((thread) => (
    thread.id === targetThreadId
    || thread.daemonThreadId === targetThreadId
    || thread.upstreamThreadId === targetThreadId
  ));
}
