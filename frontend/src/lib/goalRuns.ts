export type GoalRunStatus =
    | "queued"
    | "planning"
    | "running"
    | "awaiting_approval"
    | "paused"
    | "completed"
    | "failed"
    | "cancelled";

export type GoalRunControlAction = "pause" | "resume" | "cancel" | "retry_step" | "rerun_from_step";

export type GoalRunStepKind = "reason" | "command" | "research" | "memory" | "skill" | "unknown";

export type TodoStatus = "pending" | "in_progress" | "completed" | "blocked";

export interface TodoItem {
    id: string;
    content: string;
    status: TodoStatus;
    position: number;
    step_index?: number | null;
    created_at?: number | null;
    updated_at?: number | null;
}

export interface GoalRunStep {
    id: string;
    title: string;
    kind: GoalRunStepKind;
    status?: string | null;
    success_condition?: string | null;
    session_id?: string | null;
}

export interface GoalRunEvent {
    id: string;
    timestamp: number;
    phase: string;
    message: string;
    details?: string | null;
    step_index?: number | null;
    todo_snapshot: TodoItem[];
}

export interface GoalRun {
    id: string;
    title: string;
    goal: string;
    client_request_id?: string | null;
    status: GoalRunStatus;
    priority?: string | null;
    created_at: number;
    started_at?: number | null;
    completed_at?: number | null;
    thread_id?: string | null;
    current_step_index?: number | null;
    current_step_title?: string | null;
    current_step_kind?: GoalRunStepKind | null;
    replan_count: number;
    plan_summary?: string | null;
    reflection_summary?: string | null;
    result?: string | null;
    error?: string | null;
    last_error?: string | null;
    failure_cause?: string | null;
    memory_updates?: string[];
    generated_skill_path?: string | null;
    child_task_ids?: string[];
    child_task_count?: number | null;
    approval_count?: number | null;
    duration_ms?: number | null;
    session_id?: string | null;
    awaiting_approval_id?: string | null;
    active_task_id?: string | null;
    steps?: GoalRunStep[];
    events?: GoalRunEvent[];
}

export interface StartGoalRunPayload {
    goal: string;
    title?: string | null;
    sessionId?: string | null;
    priority?: string | null;
    threadId?: string | null;
    clientRequestId?: string | null;
}

import { getBridge } from "./bridge";

function toStepKind(value: unknown): GoalRunStepKind {
    if (typeof value !== "string") {
        return "unknown";
    }

    switch (value) {
        case "reason":
        case "command":
        case "research":
        case "memory":
        case "skill":
            return value;
        default:
            return "unknown";
    }
}

function toStatus(value: unknown): GoalRunStatus {
    if (typeof value !== "string") {
        return "queued";
    }

    switch (value) {
        case "queued":
        case "planning":
        case "running":
        case "awaiting_approval":
        case "paused":
        case "completed":
        case "failed":
        case "cancelled":
            return value;
        default:
            return "queued";
    }
}

function toStringArray(value: unknown): string[] {
    return Array.isArray(value)
        ? value.filter((entry): entry is string => typeof entry === "string" && entry.trim().length > 0)
        : [];
}

function toNumberOrNull(value: unknown): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function toTodoStatus(value: unknown): TodoStatus {
    switch (value) {
        case "in_progress":
        case "completed":
        case "blocked":
            return value;
        default:
            return "pending";
    }
}

function normalizeTodoItem(raw: unknown, index: number): TodoItem {
    const item = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    return {
        id: typeof item.id === "string" && item.id ? item.id : `todo-${index}`,
        content: typeof item.content === "string" && item.content
            ? item.content
            : typeof item.title === "string" && item.title
                ? item.title
                : `Todo ${index + 1}`,
        status: toTodoStatus(item.status),
        position: typeof item.position === "number" ? item.position : index,
        step_index: toNumberOrNull(item.step_index ?? item.stepIndex),
        created_at: toNumberOrNull(item.created_at ?? item.createdAt),
        updated_at: toNumberOrNull(item.updated_at ?? item.updatedAt),
    };
}

function normalizeEvent(raw: unknown, index: number): GoalRunEvent {
    const event = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    const todoSnapshotRaw = Array.isArray(event.todo_snapshot)
        ? event.todo_snapshot
        : Array.isArray(event.todoSnapshot)
            ? event.todoSnapshot
            : [];
    return {
        id: typeof event.id === "string" && event.id ? event.id : `goal-event-${index}`,
        timestamp: typeof event.timestamp === "number" ? event.timestamp : Date.now(),
        phase: typeof event.phase === "string" && event.phase ? event.phase : "event",
        message: typeof event.message === "string" && event.message ? event.message : "Goal updated",
        details: typeof event.details === "string" ? event.details : null,
        step_index: toNumberOrNull(event.step_index ?? event.stepIndex),
        todo_snapshot: todoSnapshotRaw.map((item, todoIndex) => normalizeTodoItem(item, todoIndex)),
    };
}

function normalizeStep(raw: unknown, index: number): GoalRunStep {
    const step = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    return {
        id: typeof step.id === "string" && step.id ? step.id : `step-${index}`,
        title: typeof step.title === "string" && step.title ? step.title : `Step ${index + 1}`,
        kind: toStepKind(step.kind),
        status: typeof step.status === "string" ? step.status : null,
        success_condition: typeof step.success_condition === "string"
            ? step.success_condition
            : typeof step.success_criteria === "string"
                ? step.success_criteria
            : typeof step.successCondition === "string"
                ? step.successCondition
                : null,
        session_id: typeof step.session_id === "string"
            ? step.session_id
            : typeof step.sessionId === "string"
                ? step.sessionId
                : null,
    };
}

export function normalizeGoalRun(raw: unknown): GoalRun | null {
    if (!raw || typeof raw !== "object") {
        return null;
    }

    const goalRun = raw as Record<string, unknown>;
    const id = typeof goalRun.id === "string" ? goalRun.id : "";
    const goal = typeof goalRun.goal === "string"
        ? goalRun.goal
        : typeof goalRun.prompt === "string"
            ? goalRun.prompt
            : "";

    if (!id || !goal) {
        return null;
    }

    const stepsRaw = Array.isArray(goalRun.steps) ? goalRun.steps : [];
    const normalizedSteps = stepsRaw.map((step, index) => normalizeStep(step, index));
    const eventsRaw = Array.isArray(goalRun.events) ? goalRun.events : [];
    const normalizedEvents = eventsRaw.map((event, index) => normalizeEvent(event, index));
    const currentStepIndex = toNumberOrNull(goalRun.current_step_index ?? goalRun.currentStepIndex);
    const derivedCurrentStep = typeof currentStepIndex === "number" ? normalizedSteps[currentStepIndex] : null;
    const currentStepKindRaw = goalRun.current_step_kind ?? goalRun.currentStepKind ?? derivedCurrentStep?.kind;

    return {
        id,
        title: typeof goalRun.title === "string" && goalRun.title ? goalRun.title : goal.slice(0, 72),
        goal,
        client_request_id: typeof goalRun.client_request_id === "string"
            ? goalRun.client_request_id
            : typeof goalRun.clientRequestId === "string"
                ? goalRun.clientRequestId
                : null,
        status: toStatus(goalRun.status),
        priority: typeof goalRun.priority === "string" ? goalRun.priority : null,
        created_at: typeof goalRun.created_at === "number" ? goalRun.created_at : Date.now(),
        started_at: toNumberOrNull(goalRun.started_at),
        completed_at: toNumberOrNull(goalRun.completed_at),
        thread_id: typeof goalRun.thread_id === "string"
            ? goalRun.thread_id
            : typeof goalRun.threadId === "string"
                ? goalRun.threadId
                : null,
        current_step_index: currentStepIndex,
        current_step_title: typeof goalRun.current_step_title === "string"
            ? goalRun.current_step_title
            : typeof goalRun.currentStepTitle === "string"
                ? goalRun.currentStepTitle
                : derivedCurrentStep?.title ?? null,
        current_step_kind: currentStepKindRaw == null ? null : toStepKind(currentStepKindRaw),
        replan_count: typeof goalRun.replan_count === "number"
            ? goalRun.replan_count
            : typeof goalRun.replanCount === "number"
                ? goalRun.replanCount
                : 0,
        plan_summary: typeof goalRun.plan_summary === "string"
            ? goalRun.plan_summary
            : typeof goalRun.planSummary === "string"
                ? goalRun.planSummary
                : null,
        reflection_summary: typeof goalRun.reflection_summary === "string"
            ? goalRun.reflection_summary
            : typeof goalRun.reflectionSummary === "string"
                ? goalRun.reflectionSummary
                : null,
        result: typeof goalRun.result === "string" ? goalRun.result : null,
        error: typeof goalRun.error === "string" ? goalRun.error : null,
        last_error: typeof goalRun.last_error === "string"
            ? goalRun.last_error
            : typeof goalRun.lastError === "string"
                ? goalRun.lastError
                : null,
        failure_cause: typeof goalRun.failure_cause === "string"
            ? goalRun.failure_cause
            : typeof goalRun.failureCause === "string"
                ? goalRun.failureCause
                : null,
        memory_updates: toStringArray(goalRun.memory_updates ?? goalRun.memoryUpdates),
        generated_skill_path: typeof goalRun.generated_skill_path === "string"
            ? goalRun.generated_skill_path
            : typeof goalRun.generatedSkillPath === "string"
                ? goalRun.generatedSkillPath
                : null,
        child_task_ids: toStringArray(goalRun.child_task_ids ?? goalRun.childTaskIds),
        child_task_count: toNumberOrNull(goalRun.child_task_count ?? goalRun.childTaskCount),
        approval_count: toNumberOrNull(goalRun.approval_count ?? goalRun.approvalCount),
        duration_ms: toNumberOrNull(goalRun.duration_ms ?? goalRun.durationMs),
        session_id: typeof goalRun.session_id === "string"
            ? goalRun.session_id
            : typeof goalRun.sessionId === "string"
                ? goalRun.sessionId
                : null,
        awaiting_approval_id: typeof goalRun.awaiting_approval_id === "string"
            ? goalRun.awaiting_approval_id
            : typeof goalRun.awaitingApprovalId === "string"
                ? goalRun.awaitingApprovalId
                : null,
        active_task_id: typeof goalRun.active_task_id === "string"
            ? goalRun.active_task_id
            : typeof goalRun.activeTaskId === "string"
                ? goalRun.activeTaskId
                : null,
        steps: normalizedSteps,
        events: normalizedEvents,
    };
}

export function normalizeGoalRunList(raw: unknown): GoalRun[] {
    if (!Array.isArray(raw)) {
        return [];
    }

    return raw
        .map((entry) => normalizeGoalRun(entry))
        .filter((entry): entry is GoalRun => Boolean(entry))
        .sort((a, b) => (b.created_at || 0) - (a.created_at || 0));
}

export function goalRunSupportAvailable(): boolean {
    const bridge = getBridge();
    return Boolean(bridge?.agentListGoalRuns && bridge?.agentStartGoalRun && bridge?.agentControlGoalRun);
}

export async function fetchGoalRuns(): Promise<GoalRun[]> {
    const bridge = getBridge();
    if (!bridge?.agentListGoalRuns) {
        return [];
    }

    try {
        const result = await bridge.agentListGoalRuns();
        return normalizeGoalRunList(result);
    } catch {
        return [];
    }
}

export async function startGoalRun(payload: StartGoalRunPayload): Promise<GoalRun | null> {
    const bridge = getBridge();
    if (!bridge?.agentStartGoalRun) {
        return null;
    }

    try {
        const result = await bridge.agentStartGoalRun({
            ...payload,
            clientRequestId: payload.clientRequestId
                ?? (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
                    ? crypto.randomUUID()
                    : `goal-run-${Date.now()}`),
        });
        return normalizeGoalRun(result);
    } catch {
        return null;
    }
}

export async function controlGoalRun(goalRunId: string, action: GoalRunControlAction, stepIndex?: number | null): Promise<boolean> {
    const bridge = getBridge();
    if (!bridge?.agentControlGoalRun) {
        return false;
    }

    try {
        const result = await bridge.agentControlGoalRun(goalRunId, action, stepIndex ?? null);
        if (typeof result === "boolean") {
            return result;
        }
        if (result && typeof result === "object") {
            const payload = result as Record<string, unknown>;
            if (typeof payload.ok === "boolean") {
                return payload.ok;
            }
            if (typeof payload.success === "boolean") {
                return payload.success;
            }
        }
        return true;
    } catch {
        return false;
    }
}

export function isGoalRunTerminal(goalRun: GoalRun): boolean {
    return goalRun.status === "completed" || goalRun.status === "failed" || goalRun.status === "cancelled";
}

export function isGoalRunActive(goalRun: GoalRun): boolean {
    return !isGoalRunTerminal(goalRun);
}

export function formatGoalRunStatus(status: GoalRunStatus): string {
    switch (status) {
        case "awaiting_approval":
            return "Awaiting approval";
        default:
            return status.replace(/_/g, " ");
    }
}

export function goalRunStatusColor(status: GoalRunStatus): string {
    switch (status) {
        case "planning":
            return "var(--mission)";
        case "running":
            return "var(--accent)";
        case "awaiting_approval":
            return "var(--approval)";
        case "paused":
            return "var(--warning)";
        case "completed":
            return "var(--success)";
        case "failed":
            return "var(--danger)";
        case "cancelled":
            return "var(--text-muted)";
        default:
            return "var(--text-secondary)";
    }
}

export function summarizeGoalRunStep(goalRun: GoalRun): string {
    if (goalRun.current_step_title) {
        return goalRun.current_step_title;
    }
    if (typeof goalRun.current_step_index === "number" && goalRun.steps?.length) {
        const step = goalRun.steps[goalRun.current_step_index];
        if (step?.title) {
            return step.title;
        }
    }
    return goalRun.status === "planning" ? "Building plan" : "Idle";
}

export function goalRunChildTaskCount(goalRun: GoalRun): number {
    if (typeof goalRun.child_task_count === "number") {
        return goalRun.child_task_count;
    }
    return goalRun.child_task_ids?.length ?? 0;
}

export function formatGoalRunDuration(durationMs?: number | null): string {
    if (typeof durationMs !== "number" || !Number.isFinite(durationMs) || durationMs <= 0) {
        return "-";
    }

    const totalSeconds = Math.max(1, Math.round(durationMs / 1000));
    if (totalSeconds < 60) {
        return `${totalSeconds}s`;
    }

    const totalMinutes = Math.round(totalSeconds / 60);
    if (totalMinutes < 60) {
        return `${totalMinutes}m`;
    }

    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
}

export function latestGoalRunTodoSnapshot(goalRun: GoalRun): TodoItem[] {
    const events = goalRun.events ?? [];
    for (let index = events.length - 1; index >= 0; index -= 1) {
        const snapshot = events[index]?.todo_snapshot ?? [];
        if (snapshot.length > 0) {
            return snapshot;
        }
    }
    return [];
}
