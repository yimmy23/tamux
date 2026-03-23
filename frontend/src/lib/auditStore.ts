import { create } from "zustand";
import type { ActionType, AuditEntry, AuditFilters, EscalationInfo, TimeRange } from "./types";

const MAX_AUDIT_ENTRIES = 500;
const ALL_ACTION_TYPES: ActionType[] = ["heartbeat", "tool", "escalation", "skill", "subagent"];

interface AuditState {
  entries: AuditEntry[];
  filters: AuditFilters;
  isOpen: boolean;
  selectedEntryId: string | null;
  currentEscalation: EscalationInfo | null;

  addEntry: (entry: AuditEntry) => void;
  dismissEntry: (entryId: string) => void;
  setEscalation: (info: EscalationInfo | null) => void;
  setTypeFilter: (types: Set<ActionType>) => void;
  setTimeRange: (range: TimeRange) => void;
  togglePanel: () => void;
  selectEntry: (id: string | null) => void;
  clearAll: () => void;
}

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

export const useAuditStore = create<AuditState>((set) => ({
  entries: [],
  filters: {
    types: new Set(ALL_ACTION_TYPES),
    timeRange: "today" as TimeRange,
  },
  isOpen: false,
  selectedEntryId: null,
  currentEscalation: null,

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
}));

/** Get entries matching current filters. */
export function filteredEntries(state: AuditState): AuditEntry[] {
  return state.entries.filter(
    (e) =>
      state.filters.types.has(e.actionType) &&
      isInTimeRange(e.timestamp, state.filters.timeRange),
  );
}
