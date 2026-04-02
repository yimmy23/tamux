import { getBridge } from "./bridge";
import { formatTaskStatus, formatTaskTimestamp, isTaskActive, isTaskTerminal, taskStatusColor, type AgentTaskPriority, type AgentTaskStatus } from "./agentTaskQueue";

export type AgentRunKind = "task" | "subagent";
export type AgentRunClassification = "coding" | "research" | "ops" | "browser" | "messaging" | "mixed" | string;

export interface AgentRun {
    id: string;
    task_id: string;
    kind: AgentRunKind;
    classification: AgentRunClassification;
    title: string;
    description: string;
    status: AgentTaskStatus;
    priority: AgentTaskPriority;
    progress: number;
    created_at: number;
    started_at?: number | null;
    completed_at?: number | null;
    thread_id?: string | null;
    session_id?: string | null;
    workspace_id?: string | null;
    source: string;
    runtime?: string | null;
    goal_run_id?: string | null;
    goal_run_title?: string | null;
    goal_step_id?: string | null;
    goal_step_title?: string | null;
    parent_run_id?: string | null;
    parent_task_id?: string | null;
    parent_thread_id?: string | null;
    parent_title?: string | null;
    blocked_reason?: string | null;
    error?: string | null;
    result?: string | null;
    last_error?: string | null;
}

export async function fetchAgentRuns(): Promise<AgentRun[]> {
    const amux = getBridge();
    if (!amux?.agentListRuns) {
        return [];
    }

    try {
        const result = await amux.agentListRuns();
        return Array.isArray(result) ? (result as AgentRun[]) : [];
    } catch {
        return [];
    }
}

export function isRunTerminal(run: AgentRun): boolean {
    return isTaskTerminal(run);
}

export function isRunActive(run: AgentRun): boolean {
    return isTaskActive(run);
}

export function isSubagentRun(run: AgentRun): boolean {
    return run.kind === "subagent" || Boolean(run.parent_run_id || run.parent_task_id || run.parent_thread_id);
}

export function formatRunStatus(run: AgentRun): string {
    return formatTaskStatus(run);
}

export function runStatusColor(status: AgentTaskStatus): string {
    return taskStatusColor(status);
}

export function formatRunTimestamp(timestamp?: number | null): string {
    return formatTaskTimestamp(timestamp);
}
