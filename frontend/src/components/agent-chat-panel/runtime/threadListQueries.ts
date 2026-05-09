import { buildHydratedRemoteThread, type AgentThread } from "@/lib/agentStore";

type AgentListThreads = (options?: {
  agentFilter?: string | null;
}) => Promise<unknown[]>;

export async function fetchHydratedRemoteThreads(params: {
  agentListThreads: AgentListThreads;
  fallbackAgentName: string;
  agentFilter?: string | null;
}): Promise<AgentThread[]> {
  const remoteThreads = await params.agentListThreads({
    agentFilter: params.agentFilter ?? null,
  }).catch(() => []);
  if (!Array.isArray(remoteThreads)) {
    return [];
  }

  const dedupedThreads = new Map<string, AgentThread>();
  for (const remoteThread of remoteThreads) {
    const hydrated = buildHydratedRemoteThread(remoteThread ?? {}, params.fallbackAgentName);
    const daemonThreadId = hydrated?.thread.daemonThreadId;
    if (!hydrated || !daemonThreadId || dedupedThreads.has(daemonThreadId)) {
      continue;
    }
    dedupedThreads.set(daemonThreadId, hydrated.thread);
  }

  return Array.from(dedupedThreads.values()).sort((left, right) => right.updatedAt - left.updatedAt);
}
