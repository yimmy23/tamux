import { create } from "zustand";
import { getBridge } from "./bridge";

export type AgentActivityState =
    | "idle"
    | "thinking"
    | "executing_tool"
    | "waiting_for_approval"
    | "running_goal"
    | "goal_running";

interface ProviderHealth {
    provider: string;
    canExecute: boolean;
    tripCount: number;
}

interface GatewayStatus {
    platform: string;
    status: string;
    consecutiveFailures: number;
}

interface RecentAction {
    id: number;
    actionType: string;
    summary: string;
    timestamp: number;
}

interface StatusDiagnostics {
    operatorProfileSyncState: string;
    operatorProfileSyncDirty: boolean;
    operatorProfileSchedulerFallback: boolean;
}

interface StatusState {
    activity: AgentActivityState;
    activeThreadId: string | null;
    activeGoalRunId: string | null;
    activeGoalRunTitle: string | null;
    providerHealth: ProviderHealth[];
    gatewayStatuses: GatewayStatus[];
    recentActions: RecentAction[];
    diagnostics: StatusDiagnostics;
    lastUpdated: number;
    updateStatus: (data: Partial<Omit<StatusState, "updateStatus">>) => void;
}

export const useStatusStore = create<StatusState>((set) => ({
    activity: "idle",
    activeThreadId: null,
    activeGoalRunId: null,
    activeGoalRunTitle: null,
    providerHealth: [],
    gatewayStatuses: [],
    recentActions: [],
    diagnostics: {
        operatorProfileSyncState: "clean",
        operatorProfileSyncDirty: false,
        operatorProfileSchedulerFallback: false,
    },
    lastUpdated: 0,
    updateStatus: (data) => set({ ...data, lastUpdated: Date.now() }),
}));

let _pollInterval: ReturnType<typeof setInterval> | null = null;

/** Start polling daemon for status updates every 10 seconds. */
export function hydrateStatusStore(): void {
    if (_pollInterval) return;
    pollStatus();
    _pollInterval = setInterval(pollStatus, 10_000);
}

const VALID_ACTIVITIES: AgentActivityState[] = [
    "idle", "thinking", "executing_tool", "waiting_for_approval", "running_goal", "goal_running",
];

async function pollStatus(): Promise<void> {
    const bridge = getBridge();
    if (!bridge?.agentGetStatus) return;
    try {
        const status = await bridge.agentGetStatus();
        if (!status) return;

        const { updateStatus } = useStatusStore.getState();

        const activity = VALID_ACTIVITIES.includes(status.activity as AgentActivityState)
            ? (status.activity as AgentActivityState)
            : "idle";

        // Transform provider health from object map to array
        const providerHealth: ProviderHealth[] = [];
        if (status.provider_health && typeof status.provider_health === "object") {
            for (const [provider, info] of Object.entries(status.provider_health)) {
                providerHealth.push({
                    provider,
                    canExecute: (info as { can_execute?: boolean }).can_execute ?? true,
                    tripCount: (info as { trip_count?: number }).trip_count ?? 0,
                });
            }
        }

        // Transform gateway statuses from object map to array
        const gatewayStatuses: GatewayStatus[] = [];
        if (status.gateway_statuses && typeof status.gateway_statuses === "object") {
            for (const [platform, info] of Object.entries(status.gateway_statuses)) {
                gatewayStatuses.push({
                    platform,
                    status: (info as { status?: string }).status ?? "unknown",
                    consecutiveFailures: (info as { consecutive_failures?: number }).consecutive_failures ?? 0,
                });
            }
        }

        // Transform recent actions
        const recentActions: RecentAction[] = Array.isArray(status.recent_actions)
            ? status.recent_actions.map((a) => ({
                id: a.id,
                actionType: a.action_type,
                summary: a.summary,
                timestamp: a.timestamp,
            }))
            : [];
        const diagnosticsRaw =
            status.diagnostics && typeof status.diagnostics === "object"
                ? (status.diagnostics as Record<string, unknown>)
                : {};
        const diagnostics: StatusDiagnostics = {
            operatorProfileSyncState:
                typeof diagnosticsRaw.operator_profile_sync_state === "string"
                    ? diagnosticsRaw.operator_profile_sync_state
                    : "clean",
            operatorProfileSyncDirty:
                diagnosticsRaw.operator_profile_sync_dirty === true,
            operatorProfileSchedulerFallback:
                diagnosticsRaw.operator_profile_scheduler_fallback === true,
        };

        updateStatus({
            activity,
            activeThreadId: status.active_thread_id,
            activeGoalRunId: status.active_goal_run_id,
            activeGoalRunTitle: status.active_goal_run_title,
            providerHealth,
            gatewayStatuses,
            recentActions,
            diagnostics,
        });
    } catch (e) {
        console.warn("[status] poll failed:", e);
    }
}

/** Clean up polling interval. */
export function destroyStatusStore(): void {
    if (_pollInterval) {
        clearInterval(_pollInterval);
        _pollInterval = null;
    }
}
