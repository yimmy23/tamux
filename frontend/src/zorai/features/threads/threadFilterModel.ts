import type { AgentThread, SubAgentDefinition } from "@/lib/agentStore";

export type ThreadFilterTab = "svarog" | "rarog" | "weles" | "goals" | "workspace" | "playgrounds" | "internal" | "gateway" | `agent:${string}`;
export type DateFilterId = "all" | "today" | "7d" | "30d" | "custom";

export const fixedThreadTabs: Array<{ id: ThreadFilterTab; label: string }> = [
  { id: "svarog", label: "Svarog" },
  { id: "rarog", label: "Rarog" },
  { id: "weles", label: "Weles" },
  { id: "goals", label: "Goals" },
  { id: "workspace", label: "Workspace" },
  { id: "playgrounds", label: "Playgrounds" },
  { id: "internal", label: "Internal" },
  { id: "gateway", label: "Gateway" },
];

export const dateFilters: Array<{ id: DateFilterId; label: string }> = [
  { id: "all", label: "All" },
  { id: "today", label: "Today" },
  { id: "7d", label: "7 days" },
  { id: "30d", label: "30 days" },
  { id: "custom", label: "Range" },
];

export function filterThreads(
  threads: AgentThread[],
  options: {
    tab: ThreadFilterTab;
    dateFilter: DateFilterId;
    fromDate: string;
    toDate: string;
    goalThreadIds: Set<string>;
  },
): AgentThread[] {
  return threads.filter((thread) => matchesThreadTab(thread, options.tab, options.goalThreadIds))
    .filter((thread) => matchesThreadDate(thread, options.dateFilter, options.fromDate, options.toDate));
}

export function buildThreadFilterTabs(
  threads: AgentThread[],
  subAgents: SubAgentDefinition[],
  goalThreadIds: Set<string>,
): Array<{ id: ThreadFilterTab; label: string }> {
  const dynamic = new Map<string, string>();

  for (const subAgent of subAgents) {
    const id = normalizeAgentTabId(subAgent.id);
    if (id) dynamic.set(id, subAgent.name || displayNameForAgentId(id));
  }

  for (const thread of threads) {
    if (
      matchesThreadTab(thread, "goals", goalThreadIds)
      || matchesThreadTab(thread, "workspace", goalThreadIds)
      || matchesThreadTab(thread, "weles", goalThreadIds)
      || matchesThreadTab(thread, "rarog", goalThreadIds)
      || matchesThreadTab(thread, "playgrounds", goalThreadIds)
      || matchesThreadTab(thread, "internal", goalThreadIds)
      || matchesThreadTab(thread, "gateway", goalThreadIds)
    ) {
      continue;
    }
    const id = dynamicAgentTabId(thread.agent_name);
    if (id) dynamic.set(id, thread.agent_name || displayNameForAgentId(id));
  }

  return [
    ...fixedThreadTabs,
    ...Array.from(dynamic.entries())
      .sort((left, right) => left[1].localeCompare(right[1]))
      .map(([id, label]) => ({ id: `agent:${id}` as const, label })),
  ];
}

function matchesThreadTab(thread: AgentThread, tab: ThreadFilterTab, goalThreadIds: Set<string>): boolean {
  const haystack = [
    thread.agent_name,
    thread.title,
    thread.daemonThreadId,
    thread.upstreamThreadId,
  ].filter(Boolean).join(" ").toLowerCase();
  const isGoal = Boolean((thread.daemonThreadId && goalThreadIds.has(thread.daemonThreadId)) || goalThreadIds.has(thread.id) || haystack.includes("goal"));
  const isWorkspace = Boolean(thread.workspaceId) || haystack.includes("workspace") || haystack.includes("wtask");
  const isWeles = haystack.includes("weles");
  const isRarog = haystack.includes("rarog");
  const isPlayground = haystack.includes("playground");
  const isInternal = haystack.includes("internal") || haystack.includes("concierge") || haystack.includes("daemon");
  const isGateway = ["slack", "discord", "telegram", "whatsapp"].some((source) => haystack.includes(source));
  const agentId = canonicalThreadAgentId(thread.agent_name);

  if (tab === "goals") return isGoal;
  if (tab === "workspace") return isWorkspace;
  if (tab === "weles") return isWeles;
  if (tab === "rarog") return isRarog;
  if (tab === "playgrounds") return isPlayground;
  if (tab === "internal") return isInternal;
  if (tab === "gateway") return isGateway;
  if (tab.startsWith("agent:")) return agentId === tab.slice("agent:".length);
  return agentId === "svarog" && !isGoal && !isWorkspace && !isWeles && !isRarog && !isPlayground && !isInternal && !isGateway;
}

function dynamicAgentTabId(value: string | null | undefined): string | null {
  const agentId = canonicalThreadAgentId(value);
  if (!agentId || ["svarog", "rarog", "concierge", "weles"].includes(agentId)) {
    return null;
  }
  return agentId;
}

function canonicalThreadAgentId(value: string | null | undefined): string | null {
  const normalized = (value ?? "").trim().toLowerCase();
  if (!normalized) return null;
  if (["svarog", "swarog", "main", "main-agent", "zorai"].includes(normalized)) return "svarog";
  if (normalized === "weles") return "weles";
  if (normalized === "rarog") return "rarog";
  if (normalized === "concierge") return "concierge";
  return normalized.replace(/_builtin$/, "");
}

function normalizeAgentTabId(value: string | null | undefined): string | null {
  const normalized = (value ?? "").trim().toLowerCase();
  if (!normalized || ["svarog", "swarog", "main", "zorai", "zorai", "rarog", "concierge", "weles"].includes(normalized)) {
    return null;
  }
  return normalized.replace(/_builtin$/, "");
}

function displayNameForAgentId(agentId: string): string {
  return agentId.split(/[-_\s]+/).filter(Boolean).map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`).join(" ") || agentId;
}

function matchesThreadDate(thread: AgentThread, dateFilter: DateFilterId, fromDate: string, toDate: string): boolean {
  if (dateFilter === "all") return true;
  const updated = new Date(thread.updatedAt);
  if (Number.isNaN(updated.getTime())) return true;
  const now = new Date();
  if (dateFilter === "today") return updated.toDateString() === now.toDateString();
  if (dateFilter === "7d") return updated.getTime() >= now.getTime() - 7 * 24 * 60 * 60 * 1000;
  if (dateFilter === "30d") return updated.getTime() >= now.getTime() - 30 * 24 * 60 * 60 * 1000;
  const from = fromDate ? new Date(`${fromDate}T00:00:00`).getTime() : Number.NEGATIVE_INFINITY;
  const to = toDate ? new Date(`${toDate}T23:59:59`).getTime() : Number.POSITIVE_INFINITY;
  const value = updated.getTime();
  return value >= from && value <= to;
}
