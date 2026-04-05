import { create } from "zustand";
import { getBridge } from "./bridge";
import type { ActionType, AuditEntry, AuditFilters, EscalationInfo, TimeRange } from "./types";

const MAX_AUDIT_ENTRIES = 500;
const ALL_ACTION_TYPES: ActionType[] = ["heartbeat", "tool", "escalation", "skill", "subagent"];

interface AuditState {
  entries: AuditEntry[];
  filters: AuditFilters;
  isOpen: boolean;
  selectedEntryId: string | null;
  currentEscalation: EscalationInfo | null;
  loadingHistory: boolean;
  loadingProvenance: boolean;
  loadingMemoryProvenance: boolean;
  provenanceReport: {
    totalEntries: number;
    validHashEntries: number;
    validSignatureEntries: number;
    validChainEntries: number;
    entries: ProvenanceEntry[];
  } | null;
  memoryProvenanceReport: {
    totalEntries: number;
    summaryByStatus: Record<string, number>;
  } | null;
  provenanceStatus: string | null;
  memoryProvenanceStatus: string | null;

  addEntry: (entry: AuditEntry) => void;
  dismissEntry: (entryId: string) => void;
  setEscalation: (info: EscalationInfo | null) => void;
  setTypeFilter: (types: Set<ActionType>) => void;
  setTimeRange: (range: TimeRange) => void;
  togglePanel: () => void;
  selectEntry: (id: string | null) => void;
  clearAll: () => void;
  loadAuditHistory: () => Promise<void>;
  loadProvenanceReport: (limit?: number) => Promise<void>;
  loadMemoryProvenanceReport: (target?: string, limit?: number) => Promise<void>;
}

export type ProvenanceEntry = {
  eventType: string;
  summary: string;
  complianceMode: string | null;
  goalRunId: string | null;
  taskId: string | null;
  causalTraceId: string | null;
  hashValid: boolean;
  signatureValid: boolean;
  chainValid: boolean;
};

type AuditEntryWire = {
  id: string;
  timestamp: number;
  action_type: string;
  summary: string;
  explanation?: string | null;
  confidence?: number | null;
  confidence_band?: string | null;
  causal_trace_id?: string | null;
  thread_id?: string | null;
  goal_run_id?: string | null;
  task_id?: string | null;
};

function isInTimeRange(timestamp: number, range: TimeRange): boolean {
  const now = Date.now();
  switch (range) {
    case "last_hour": return timestamp >= now - 3600_000;
    case "today": {
      const startOfDay = new Date();
      startOfDay.setHours(0, 0, 0, 0);
      return timestamp >= startOfDay.getTime();
    }
    case "this_week": return timestamp >= now - 7 * 86400_000;
    case "all_time": return true;
  }
}

const ACTION_TYPE_SET = new Set<ActionType>(ALL_ACTION_TYPES);

function normalizeActionType(value: string): ActionType {
  return ACTION_TYPE_SET.has(value as ActionType) ? (value as ActionType) : "tool";
}

function mapAuditEntry(entry: AuditEntryWire): AuditEntry {
  return {
    id: entry.id,
    timestamp: entry.timestamp,
    actionType: normalizeActionType(entry.action_type),
    summary: entry.summary,
    explanation: entry.explanation ?? null,
    confidence: entry.confidence ?? null,
    confidenceBand: entry.confidence_band ?? null,
    causalTraceId: entry.causal_trace_id ?? null,
    threadId: entry.thread_id ?? null,
    goalRunId: entry.goal_run_id ?? null,
    taskId: entry.task_id ?? null,
  };
}

function mergeEntries(existing: AuditEntry[], incoming: AuditEntry[]): AuditEntry[] {
  const byId = new Map<string, AuditEntry>();
  for (const entry of [...incoming, ...existing]) {
    byId.set(entry.id, entry);
  }
  return [...byId.values()]
    .sort((left, right) => right.timestamp - left.timestamp)
    .slice(0, MAX_AUDIT_ENTRIES);
}

function mapProvenanceEntry(entry: {
  event_type?: string;
  summary?: string;
  compliance_mode?: string | null;
  goal_run_id?: string | null;
  task_id?: string | null;
  causal_trace_id?: string | null;
  hash_valid?: boolean;
  signature_valid?: boolean;
  chain_valid?: boolean;
}): ProvenanceEntry {
  return {
    eventType: typeof entry.event_type === "string" ? entry.event_type : "",
    summary: typeof entry.summary === "string" ? entry.summary : "",
    complianceMode: typeof entry.compliance_mode === "string" ? entry.compliance_mode : null,
    goalRunId: typeof entry.goal_run_id === "string" ? entry.goal_run_id : null,
    taskId: typeof entry.task_id === "string" ? entry.task_id : null,
    causalTraceId: typeof entry.causal_trace_id === "string" ? entry.causal_trace_id : null,
    hashValid: entry.hash_valid !== false,
    signatureValid: entry.signature_valid !== false,
    chainValid: entry.chain_valid !== false,
  };
}

export function findMatchingProvenanceEntry(
  entry: AuditEntry,
  report: AuditState["provenanceReport"],
): ProvenanceEntry | null {
  if (!report) {
    return null;
  }

  if (entry.causalTraceId) {
    const direct = report.entries.find(
      (candidate) => candidate.causalTraceId === entry.causalTraceId,
    );
    if (direct) {
      return direct;
    }
  }

  if (entry.goalRunId || entry.taskId) {
    return (
      report.entries.find(
        (candidate) =>
          candidate.goalRunId === (entry.goalRunId ?? null)
          && candidate.taskId === (entry.taskId ?? null),
      ) ?? null
    );
  }

  return null;
}

function sinceForRange(range: TimeRange): number | null {
  const now = Date.now();
  switch (range) {
    case "last_hour":
      return now - 3600_000;
    case "today": {
      const startOfDay = new Date();
      startOfDay.setHours(0, 0, 0, 0);
      return startOfDay.getTime();
    }
    case "this_week":
      return now - 7 * 86400_000;
    case "all_time":
      return null;
  }
}

export const useAuditStore = create<AuditState>((set) => ({
  entries: [],
  filters: {
    types: new Set(ALL_ACTION_TYPES),
    timeRange: "today" as TimeRange,
  },
  isOpen: false,
  selectedEntryId: null,
  currentEscalation: null,
  loadingHistory: false,
  loadingProvenance: false,
  loadingMemoryProvenance: false,
  provenanceReport: null,
  memoryProvenanceReport: null,
  provenanceStatus: null,
  memoryProvenanceStatus: null,

  addEntry: (entry) =>
    set((s) => ({
      entries: [entry, ...s.entries].slice(0, MAX_AUDIT_ENTRIES),
    })),

  dismissEntry: (entryId) => {
    set((s) => ({
      entries: s.entries.map((e) =>
        e.id === entryId ? { ...e, userAction: "dismissed" as const } : e
      ),
    }));
    // Send dismiss to daemon via IPC bridge
    const w = window as unknown as Record<string, unknown>;
    const bridge = w.tamux ?? w.amux;
    if (bridge && typeof (bridge as Record<string, unknown>).dismissAuditEntry === "function") {
      (bridge as Record<string, (...args: unknown[]) => unknown>).dismissAuditEntry(entryId);
    }
  },

  setEscalation: (info) => set({ currentEscalation: info }),

  setTypeFilter: (types) =>
    set((s) => ({
      filters: { ...s.filters, types },
    })),

  setTimeRange: (range) =>
    set((s) => ({
      filters: { ...s.filters, timeRange: range },
    })),

  togglePanel: () => set((s) => ({ isOpen: !s.isOpen })),

  selectEntry: (id) => set({ selectedEntryId: id }),

  clearAll: () =>
    set({
      entries: [],
      selectedEntryId: null,
      currentEscalation: null,
    }),

  loadAuditHistory: async () => {
    const bridge = getBridge();
    if (!bridge?.agentQueryAudits) {
      set({ provenanceStatus: "Audit history bridge unavailable." });
      return;
    }
    set({ loadingHistory: true });
    try {
      const state = useAuditStore.getState();
      const response = await bridge.agentQueryAudits(
        Array.from(state.filters.types),
        sinceForRange(state.filters.timeRange),
        100,
      ) as AuditEntryWire[] | { error?: string };
      if (!Array.isArray(response)) {
        throw new Error(response?.error || "Failed to load audit history.");
      }
      set((s) => ({
        entries: mergeEntries(s.entries, response.map(mapAuditEntry)),
        loadingHistory: false,
      }));
    } catch (error) {
      set({
        loadingHistory: false,
        provenanceStatus: error instanceof Error ? error.message : "Failed to load audit history.",
      });
    }
  },

  loadProvenanceReport: async (limit = 25) => {
    const bridge = getBridge();
    if (!bridge?.agentGetProvenanceReport) {
      set({ provenanceStatus: "Provenance bridge unavailable." });
      return;
    }
    set({ loadingProvenance: true });
    try {
      const response = await bridge.agentGetProvenanceReport(limit) as {
        total_entries?: number;
        valid_hash_entries?: number;
        valid_signature_entries?: number;
        valid_chain_entries?: number;
        error?: string;
      };
      if (!response || typeof response !== "object" || response.error) {
        throw new Error(response?.error || "Failed to load provenance report.");
      }
      set({
        loadingProvenance: false,
        provenanceReport: {
          totalEntries: response.total_entries ?? 0,
          validHashEntries: response.valid_hash_entries ?? 0,
          validSignatureEntries: response.valid_signature_entries ?? 0,
          validChainEntries: response.valid_chain_entries ?? 0,
          entries: Array.isArray((response as { entries?: unknown[] }).entries)
            ? (response as { entries: Array<Record<string, unknown>> }).entries.map((entry) =>
              mapProvenanceEntry(entry),
            )
            : [],
        },
        provenanceStatus: "Provenance report loaded.",
      });
    } catch (error) {
      set({
        loadingProvenance: false,
        provenanceStatus: error instanceof Error ? error.message : "Failed to load provenance report.",
      });
    }
  },

  loadMemoryProvenanceReport: async (target = "MEMORY.md", limit = 25) => {
    const bridge = getBridge();
    if (!bridge?.agentGetMemoryProvenanceReport) {
      set({ memoryProvenanceStatus: "Memory provenance bridge unavailable." });
      return;
    }
    set({ loadingMemoryProvenance: true });
    try {
      const response = await bridge.agentGetMemoryProvenanceReport(target, limit) as {
        total_entries?: number;
        summary_by_status?: Record<string, number>;
        error?: string;
      };
      if (!response || typeof response !== "object" || response.error) {
        throw new Error(response?.error || "Failed to load memory provenance report.");
      }
      set({
        loadingMemoryProvenance: false,
        memoryProvenanceReport: {
          totalEntries: response.total_entries ?? 0,
          summaryByStatus: response.summary_by_status ?? {},
        },
        memoryProvenanceStatus: `Memory provenance loaded for ${target}.`,
      });
    } catch (error) {
      set({
        loadingMemoryProvenance: false,
        memoryProvenanceStatus: error instanceof Error ? error.message : "Failed to load memory provenance report.",
      });
    }
  },
}));

/** Get entries matching current filters. */
export function filteredEntries(state: AuditState): AuditEntry[] {
  return state.entries.filter(
    (e) =>
      state.filters.types.has(e.actionType) &&
      isInTimeRange(e.timestamp, state.filters.timeRange),
  );
}
