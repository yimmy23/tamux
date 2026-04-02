import { getBridge } from "../bridge";
import { readPersistedJson, readPersistedText, scheduleTextWrite } from "../persistence";
import { useAgentStore } from "../agentStore";
import { useSnippetStore } from "../snippetStore";
import type {
  AgentEventRow,
  ApprovalRequest,
  CognitiveEvent,
  ContextSnapshot,
  MissionDbApi,
  MissionMemory,
  OperationalEvent,
  PersistedMissionState,
} from "./types";
import {
  CATEGORY_ALLOWLIST,
  CATEGORY_APPROVAL,
  CATEGORY_COGNITIVE,
  CATEGORY_CONTEXT,
  CATEGORY_OPERATIONAL,
  MAX_APPROVALS,
  MAX_COGNITIVE_EVENTS,
  MAX_CONTEXT_SNAPSHOTS,
  MAX_OPERATIONAL_EVENTS,
  MEMORY_MAX_CHARS,
  USER_MAX_CHARS,
} from "./types";

let operationalId = 0;
let cognitiveId = 0;
let contextId = 0;
let approvalId = 0;

export function getMissionDbApi(): MissionDbApi | null {
  const api = getBridge();
  if (!api) return null;
  return api as MissionDbApi;
}

export function limitItems<T>(items: T[], maxItems: number): T[] {
  return items.slice(0, maxItems);
}

export function nextId(prefix: string, counter: "operational" | "cognitive" | "context" | "approval") {
  if (counter === "operational") return `${prefix}_${++operationalId}`;
  if (counter === "cognitive") return `${prefix}_${++cognitiveId}`;
  if (counter === "context") return `${prefix}_${++contextId}`;
  return `${prefix}_${++approvalId}`;
}

export function trimBoundedText(text: string, maxChars: number): string {
  return text.slice(0, maxChars).trimEnd();
}

export function defaultFrozenSnapshot(): string {
  return trimBoundedText(
    [
      "Environment facts:",
      "- tamux uses a daemon-first PTY backend with persistent sessions.",
      "- The frontend exposes agent traces, approvals, and execution graphs.",
      "- Snippets act as portable procedural skills for repeated workflows.",
      "- Risky shell commands require explicit approval before Enter is sent.",
    ].join("\n"),
    MEMORY_MAX_CHARS,
  );
}

export function defaultUserProfile(): string {
  return trimBoundedText(
    [
      "Operator profile:",
      "- Prefer concise, high-signal operational summaries.",
      "- Show traces, blast radius, and next action before risky execution.",
    ].join("\n"),
    USER_MAX_CHARS,
  );
}

function stripAnsi(text: string): string {
  return text
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\u001b\][^\u0007]*(?:\u0007|\u001b\\)/g, "");
}

export function extractCognitiveSegments(text: string): Array<{ source: "inner-monologue" | "scratchpad"; content: string }> {
  if (!text.includes("<INNER_MONOLOGUE>") && !text.includes("<SCRATCHPAD>")) {
    return [];
  }

  const cleaned = stripAnsi(text);
  const matches: Array<{ source: "inner-monologue" | "scratchpad"; content: string }> = [];
  const patterns: Array<{ source: "inner-monologue" | "scratchpad"; regex: RegExp }> = [
    { source: "inner-monologue", regex: /<INNER_MONOLOGUE>([\s\S]*?)<\/INNER_MONOLOGUE>/gi },
    { source: "scratchpad", regex: /<SCRATCHPAD>([\s\S]*?)<\/SCRATCHPAD>/gi },
  ];

  for (const pattern of patterns) {
    for (const match of cleaned.matchAll(pattern.regex)) {
      const content = match[1]?.trim();
      if (content) {
        matches.push({ source: pattern.source, content });
      }
    }
  }

  return matches;
}

export function syncCounters(state: PersistedMissionState): void {
  for (const event of state.operationalEvents ?? []) {
    const match = /^op_(\d+)$/.exec(event.id);
    if (match) operationalId = Math.max(operationalId, Number(match[1]));
  }
  for (const event of state.cognitiveEvents ?? []) {
    const match = /^cog_(\d+)$/.exec(event.id);
    if (match) cognitiveId = Math.max(cognitiveId, Number(match[1]));
  }
  for (const snapshot of state.contextSnapshots ?? []) {
    const match = /^ctx_(\d+)$/.exec(snapshot.id);
    if (match) contextId = Math.max(contextId, Number(match[1]));
  }
  for (const approval of state.approvals ?? []) {
    const match = /^apr_(\d+)$/.exec(approval.id);
    if (match) approvalId = Math.max(approvalId, Number(match[1]));
  }
}

export function serializeOperationalEvent(event: OperationalEvent): AgentEventRow {
  return {
    id: event.id,
    category: CATEGORY_OPERATIONAL,
    kind: event.kind,
    pane_id: event.paneId,
    workspace_id: event.workspaceId,
    surface_id: event.surfaceId,
    session_id: event.sessionId,
    payload_json: JSON.stringify(event),
    timestamp: event.timestamp,
  };
}

export function serializeCognitiveEvent(event: CognitiveEvent): AgentEventRow {
  return {
    id: event.id,
    category: CATEGORY_COGNITIVE,
    kind: event.source,
    pane_id: event.paneId,
    workspace_id: event.workspaceId,
    surface_id: event.surfaceId,
    session_id: event.sessionId,
    payload_json: JSON.stringify(event),
    timestamp: event.timestamp,
  };
}

export function serializeContextSnapshot(snapshot: ContextSnapshot): AgentEventRow {
  return {
    id: snapshot.id,
    category: CATEGORY_CONTEXT,
    kind: "context-snapshot",
    pane_id: snapshot.paneId,
    workspace_id: snapshot.workspaceId,
    surface_id: snapshot.surfaceId,
    session_id: snapshot.sessionId,
    payload_json: JSON.stringify(snapshot),
    timestamp: snapshot.timestamp,
  };
}

export function serializeApprovalRequest(approval: ApprovalRequest): AgentEventRow {
  return {
    id: approval.id,
    category: CATEGORY_APPROVAL,
    kind: approval.status,
    pane_id: approval.paneId,
    workspace_id: approval.workspaceId,
    surface_id: approval.surfaceId,
    session_id: approval.sessionId,
    payload_json: JSON.stringify(approval),
    timestamp: approval.createdAt,
  };
}

export function serializeAllowlistEntry(sessionKey: string, commands: string[]): AgentEventRow {
  return {
    id: `allow_${sessionKey}`,
    category: CATEGORY_ALLOWLIST,
    kind: "session-command-allowlist",
    pane_id: sessionKey,
    workspace_id: null,
    surface_id: null,
    session_id: sessionKey,
    payload_json: JSON.stringify({ sessionKey, commands }),
    timestamp: Date.now(),
  };
}

function parseRows(rows: unknown): AgentEventRow[] {
  if (!Array.isArray(rows)) return [];
  return rows.filter((row): row is AgentEventRow => {
    if (!row || typeof row !== "object") return false;
    const candidate = row as Partial<AgentEventRow>;
    return typeof candidate.id === "string"
      && typeof candidate.category === "string"
      && typeof candidate.kind === "string"
      && typeof candidate.payload_json === "string"
      && typeof candidate.timestamp === "number";
  });
}

function parsePayload<T>(row: AgentEventRow): T | null {
  try {
    return JSON.parse(row.payload_json) as T;
  } catch {
    return null;
  }
}

export async function loadDbMissionState(): Promise<PersistedMissionState | null> {
  const api = getMissionDbApi();
  if (!api?.dbListAgentEvents) return null;

  const [operationalRows, cognitiveRows, contextRows, approvalRows, allowlistRows] = await Promise.all([
    api.dbListAgentEvents({ category: CATEGORY_OPERATIONAL, limit: MAX_OPERATIONAL_EVENTS }),
    api.dbListAgentEvents({ category: CATEGORY_COGNITIVE, limit: MAX_COGNITIVE_EVENTS }),
    api.dbListAgentEvents({ category: CATEGORY_CONTEXT, limit: MAX_CONTEXT_SNAPSHOTS }),
    api.dbListAgentEvents({ category: CATEGORY_APPROVAL, limit: MAX_APPROVALS }),
    api.dbListAgentEvents({ category: CATEGORY_ALLOWLIST, limit: 500 }),
  ]);

  const operationalEvents = parseRows(operationalRows).map((row) => parsePayload<OperationalEvent>(row)).filter((row): row is OperationalEvent => Boolean(row));
  const cognitiveEvents = parseRows(cognitiveRows).map((row) => parsePayload<CognitiveEvent>(row)).filter((row): row is CognitiveEvent => Boolean(row));
  const contextSnapshots = parseRows(contextRows).map((row) => parsePayload<ContextSnapshot>(row)).filter((row): row is ContextSnapshot => Boolean(row));
  const approvals = parseRows(approvalRows).map((row) => parsePayload<ApprovalRequest>(row)).filter((row): row is ApprovalRequest => Boolean(row));

  const sessionAllowlist = parseRows(allowlistRows).reduce<Record<string, string[]>>((acc, row) => {
    const payload = parsePayload<{ sessionKey: string; commands: string[] }>(row);
    if (!payload || typeof payload.sessionKey !== "string" || !Array.isArray(payload.commands)) {
      return acc;
    }
    acc[payload.sessionKey] = payload.commands.filter((command): command is string => typeof command === "string");
    return acc;
  }, {});

  if (operationalEvents.length === 0 && cognitiveEvents.length === 0 && contextSnapshots.length === 0 && approvals.length === 0 && Object.keys(sessionAllowlist).length === 0) {
    return null;
  }

  return {
    operationalEvents,
    cognitiveEvents,
    contextSnapshots,
    approvals,
    sessionAllowlist,
  };
}

export function buildContextSnapshot(opts: {
  paneId: string;
  workspaceId?: string | null;
  surfaceId?: string | null;
  sessionId?: string | null;
  frozenSnapshot: string;
  userProfile: string;
}): ContextSnapshot {
  const { agentSettings, threads } = useAgentStore.getState();
  const snippets = useSnippetStore.getState().snippets;
  const activeProviderConfig = agentSettings[agentSettings.active_provider] as { model: string };

  return {
    id: nextId("ctx", "context"),
    timestamp: Date.now(),
    paneId: opts.paneId,
    workspaceId: opts.workspaceId ?? null,
    surfaceId: opts.surfaceId ?? null,
    sessionId: opts.sessionId ?? null,
    active_provider: agentSettings.active_provider,
    model: activeProviderConfig?.model ?? "",
    threadCount: threads.length,
    snippetCount: snippets.length,
    tokenBudget: agentSettings.context_budget_tokens,
    systemMemoryChars: opts.frozenSnapshot.length,
    userMemoryChars: opts.userProfile.length,
  };
}

export function persistSingleMissionEvent(row: AgentEventRow): void {
  const api = getMissionDbApi();
  if (!api?.dbUpsertAgentEvent) return;
  void api.dbUpsertAgentEvent(row);
}

export function persistAllowlist(sessionAllowlist: Record<string, string[]>): void {
  const api = getMissionDbApi();
  if (!api?.dbUpsertAgentEvent) return;
  void (async () => {
    for (const [sessionKey, commands] of Object.entries(sessionAllowlist)) {
      await api.dbUpsertAgentEvent?.(serializeAllowlistEntry(sessionKey, commands));
    }
  })();
}

export function persistMissionState(state: PersistedMissionState): void {
  const payload: PersistedMissionState = {
    operationalEvents: limitItems(state.operationalEvents ?? [], MAX_OPERATIONAL_EVENTS),
    cognitiveEvents: limitItems(state.cognitiveEvents ?? [], MAX_COGNITIVE_EVENTS),
    contextSnapshots: limitItems(state.contextSnapshots ?? [], MAX_CONTEXT_SNAPSHOTS),
    approvals: limitItems(state.approvals ?? [], MAX_APPROVALS),
    sessionAllowlist: state.sessionAllowlist ?? {},
  };
  const api = getMissionDbApi();
  if (!api?.dbUpsertAgentEvent) return;

  void (async () => {
    for (const event of payload.operationalEvents ?? []) {
      await api.dbUpsertAgentEvent?.(serializeOperationalEvent(event));
    }
    for (const event of payload.cognitiveEvents ?? []) {
      await api.dbUpsertAgentEvent?.(serializeCognitiveEvent(event));
    }
    for (const snapshot of payload.contextSnapshots ?? []) {
      await api.dbUpsertAgentEvent?.(serializeContextSnapshot(snapshot));
    }
    for (const approval of payload.approvals ?? []) {
      await api.dbUpsertAgentEvent?.(serializeApprovalRequest(approval));
    }
    for (const [sessionKey, commands] of Object.entries(payload.sessionAllowlist ?? {})) {
      await api.dbUpsertAgentEvent?.(serializeAllowlistEntry(sessionKey, commands));
    }
  })();
}

export {
  readPersistedJson,
  readPersistedText,
  scheduleTextWrite,
};

export type HydratedMemory = Partial<MissionMemory>;
