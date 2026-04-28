import type { AgentMessage, AgentThread } from "@/lib/agentStore";
import type { GoalRun } from "@/lib/goalRuns";

export type UsageRow = {
  key: string;
  provider: string;
  model: string;
  requests: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  reasoningTokens: number;
  audioTokens: number;
  videoTokens: number;
  cost: number;
  avgTps: number;
  tpsSamples: number;
};

export type SessionUsageRow = UsageRow & {
  threadId: string;
  title: string;
  updatedAt: number;
  providerModels: Set<string>;
};

export type GoalUsageRow = {
  key: string;
  goal: string;
  status: string;
  provider: string;
  model: string;
  requests: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  cost: number;
  durationMs: number;
};

export type UsageStats = {
  totals: {
    sessions: number;
    requests: number;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    reasoningTokens: number;
    audioTokens: number;
    videoTokens: number;
    cost: number;
    avgTps: number;
  };
  providerRows: UsageRow[];
  sessionRows: SessionUsageRow[];
  goalRows: GoalUsageRow[];
};

export function buildUsageStats(
  threads: AgentThread[],
  messagesByThread: Record<string, AgentMessage[]>,
  goals: GoalRun[],
): UsageStats {
  const providerMap = new Map<string, UsageRow>();
  const sessionRows: SessionUsageRow[] = [];
  const goalRows: GoalUsageRow[] = [];
  let tpsSum = 0;
  let tpsSamples = 0;

  for (const thread of threads) {
    const session = createSessionRow(thread);
    for (const message of messagesByThread[thread.id] ?? []) {
      if (message.role !== "assistant" || ((message.totalTokens ?? 0) <= 0 && message.cost === undefined)) continue;
      const provider = String(message.provider || thread.upstreamProvider || "unknown");
      const model = String(message.model || thread.upstreamModel || "unknown");
      const key = `${provider}/${model}`;
      const providerRow = providerMap.get(key) ?? createUsageRow(key, provider, model);
      addMessageUsage(providerRow, message);
      addMessageUsage(session, message);
      session.providerModels.add(key);
      providerMap.set(key, providerRow);
      if (typeof message.tps === "number" && Number.isFinite(message.tps) && message.tps > 0) {
        tpsSum += message.tps;
        tpsSamples += 1;
      }
    }
    if (session.requests > 0) sessionRows.push(session);
  }

  for (const goal of goals) {
    for (const usage of goal.model_usage ?? []) {
      const prompt = Math.max(0, Number(usage.prompt_tokens ?? 0));
      const completion = Math.max(0, Number(usage.completion_tokens ?? 0));
      goalRows.push({
        key: `${goal.id}-${usage.provider}-${usage.model}`,
        goal: goal.title || goal.goal,
        status: goal.status,
        provider: usage.provider || "unknown",
        model: usage.model || "unknown",
        requests: Math.max(0, Number(usage.request_count ?? 0)),
        promptTokens: prompt,
        completionTokens: completion,
        totalTokens: prompt + completion,
        cost: Math.max(0, Number(usage.estimated_cost_usd ?? 0)),
        durationMs: Math.max(0, Number(usage.duration_ms ?? 0)),
      });
    }
  }

  const providerRows = Array.from(providerMap.values()).map(finalizeUsageRow).sort(sortByTotalTokens);
  const finalizedSessions = sessionRows.map(finalizeUsageRow).sort(sortByTotalTokens);
  const totals = providerRows.reduce((acc, row) => {
    acc.requests += row.requests;
    acc.promptTokens += row.promptTokens;
    acc.completionTokens += row.completionTokens;
    acc.totalTokens += row.totalTokens;
    acc.reasoningTokens += row.reasoningTokens;
    acc.audioTokens += row.audioTokens;
    acc.videoTokens += row.videoTokens;
    acc.cost += row.cost;
    return acc;
  }, { sessions: finalizedSessions.length, requests: 0, promptTokens: 0, completionTokens: 0, totalTokens: 0, reasoningTokens: 0, audioTokens: 0, videoTokens: 0, cost: 0, avgTps: 0 });

  totals.avgTps = tpsSamples > 0 ? tpsSum / tpsSamples : 0;
  return { totals, providerRows, sessionRows: finalizedSessions, goalRows };
}

function createUsageRow(key: string, provider: string, model: string): UsageRow {
  return { key, provider, model, requests: 0, promptTokens: 0, completionTokens: 0, totalTokens: 0, reasoningTokens: 0, audioTokens: 0, videoTokens: 0, cost: 0, avgTps: 0, tpsSamples: 0 };
}

function createSessionRow(thread: AgentThread): SessionUsageRow {
  return { ...createUsageRow(thread.id, "thread", thread.agent_name), threadId: thread.id, title: thread.title || thread.id, updatedAt: thread.updatedAt, providerModels: new Set<string>() };
}

function addMessageUsage(row: UsageRow, message: AgentMessage) {
  const prompt = Math.max(0, Number(message.inputTokens ?? 0));
  const completion = Math.max(0, Number(message.outputTokens ?? 0));
  const total = Math.max(0, Number(message.totalTokens ?? prompt + completion));
  row.requests += 1;
  row.promptTokens += prompt;
  row.completionTokens += completion;
  row.totalTokens += total;
  row.reasoningTokens += Math.max(0, Number(message.reasoningTokens ?? 0));
  row.audioTokens += Math.max(0, Number(message.audioTokens ?? 0));
  row.videoTokens += Math.max(0, Number(message.videoTokens ?? 0));
  row.cost += Math.max(0, Number(message.cost ?? 0));
  if (typeof message.tps === "number" && Number.isFinite(message.tps) && message.tps > 0) {
    row.avgTps += message.tps;
    row.tpsSamples += 1;
  }
}

function finalizeUsageRow<T extends UsageRow>(row: T): T {
  row.avgTps = row.tpsSamples > 0 ? row.avgTps / row.tpsSamples : 0;
  return row;
}

function sortByTotalTokens(a: UsageRow, b: UsageRow): number {
  return b.totalTokens - a.totalTokens;
}

export function formatCount(value: number): string {
  return Math.round(value).toLocaleString();
}

export function formatCost(value: number): string {
  return `$${value.toFixed(value >= 1 ? 2 : 6)}`;
}

export function formatDate(timestamp: number): string {
  return Number.isFinite(timestamp) ? new Date(timestamp).toLocaleDateString() : "pending";
}

export function formatDuration(durationMs: number): string {
  if (!Number.isFinite(durationMs) || durationMs <= 0) return "n/a";
  if (durationMs < 1000) return `${durationMs}ms`;
  return `${(durationMs / 1000).toFixed(1)}s`;
}
