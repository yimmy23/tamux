import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { allLeafIds, findLeaf } from "../../lib/bspTree";
import {
    controlGoalRun,
    fetchGoalRuns,
    formatGoalRunDuration,
    formatGoalRunStatus,
    goalRunStatusColor,
    goalRunChildTaskCount,
    goalRunSupportAvailable,
    isGoalRunActive,
    latestGoalRunTodoSnapshot,
    startGoalRun,
    summarizeGoalRunStep,
    type GoalRun,
    type GoalRunEvent,
    type TodoItem,
} from "../../lib/goalRuns";
import {
    fetchAgentTasks,
    formatTaskStatus,
    formatTaskTimestamp,
    isTaskActive,
    taskStatusColor,
    type AgentQueueTask,
} from "../../lib/agentTaskQueue";
import { fetchAgentRuns, formatRunStatus, runStatusColor, type AgentRun } from "../../lib/agentRuns";
import { provisionAgentWorkspaceTerminals } from "../../lib/agentWorkspace";
import { fetchThreadTodos } from "../../lib/agentTodos";
import { fetchFilePreview, fetchGitDiff, fetchThreadWorkContext, type ThreadWorkContext, type WorkContextEntry } from "../../lib/agentWorkContext";
import { useAgentStore } from "../../lib/agentStore";
import type { Workspace } from "../../lib/types";
import { shortenHomePath, useWorkspaceStore } from "../../lib/workspaceStore";
import { EmptyPanel, SectionTitle, ActionButton, iconButtonStyle } from "./shared";

interface HeartbeatItem {
    id: string;
    label: string;
    prompt: string;
    interval_minutes: number;
    enabled: boolean;
    last_run_at: number | null;
    last_result: "ok" | "alert" | "error" | null;
    last_message: string | null;
}

const heartbeatColors: Record<string, string> = {
    ok: "var(--success)",
    alert: "var(--warning)",
    error: "var(--danger)",
};

type TaskWorkspaceLocation = {
    workspaceId: string;
    workspaceName: string;
    surfaceId: string;
    surfaceName: string;
    paneId: string;
    cwd: string | null;
};

type TasksViewProps = {
    onOpenThreadView?: () => void;
};

type RemoteAgentMessage = {
    role: "user" | "assistant" | "system" | "tool";
    content: string;
    input_tokens?: number;
    output_tokens?: number;
    reasoning?: string | null;
    tool_calls?: unknown[] | null;
    tool_name?: string | null;
    tool_call_id?: string | null;
    tool_arguments?: string | null;
    tool_status?: string | null;
    timestamp?: number;
};

type RemoteAgentThread = {
    id: string;
    title: string;
    messages: RemoteAgentMessage[];
};

type ThreadTarget = {
    title: string;
    thread_id?: string | null;
    session_id?: string | null;
};

function workContextKindLabel(entry: WorkContextEntry): string {
    const kind = entry.changeKind;
    switch (kind) {
        case "added":
            return "Added";
        case "deleted":
            return "Deleted";
        case "renamed":
            return "Renamed";
        case "copied":
            return "Copied";
        case "untracked":
            return "Untracked";
        case "conflict":
            return "Conflict";
        case "modified":
            return "Modified";
        default:
            if (entry.kind === "generated_skill") return "Skill";
            if (entry.kind === "artifact") return "Artifact";
            return "Changed";
    }
}

function workContextKindColor(entry: WorkContextEntry): string {
    switch (entry.changeKind) {
        case "added":
        case "copied":
        case "untracked":
            return "var(--success)";
        case "deleted":
            return "var(--danger)";
        case "renamed":
            return "var(--accent)";
        case "conflict":
            return "var(--warning)";
        default:
            if (entry.kind === "generated_skill") return "var(--mission)";
            if (entry.kind === "artifact") return "var(--accent)";
            return "var(--text-secondary)";
    }
}

function taskLooksLikeCoding(task: AgentQueueTask): boolean {
    const haystack = `${task.title} ${task.description} ${task.command ?? ""}`.toLowerCase();
    return /(code|coding|repo|git|diff|patch|file|files|test|build|compile|fix|bug|rust|typescript|frontend|backend|refactor|implement)/.test(haystack);
}

function findTaskWorkspaceLocation(workspaces: Workspace[], sessionId: string | null | undefined): TaskWorkspaceLocation | null {
    if (!sessionId) {
        return null;
    }

    for (const workspace of workspaces) {
        for (const surface of workspace.surfaces) {
            for (const paneId of allLeafIds(surface.layout)) {
                const leafSessionId = findLeaf(surface.layout, paneId)?.sessionId ?? null;
                const panel = surface.canvasPanels.find((entry) => entry.paneId === paneId) ?? null;
                const paneSessionId = panel?.sessionId ?? leafSessionId;
                if (paneSessionId !== sessionId) {
                    continue;
                }

                return {
                    workspaceId: workspace.id,
                    workspaceName: workspace.name,
                    surfaceId: surface.id,
                    surfaceName: surface.name,
                    paneId,
                    cwd: panel?.cwd ?? workspace.cwd ?? null,
                };
            }
        }
    }

    return null;
}

export function TasksView({ onOpenThreadView }: TasksViewProps) {
    const [tasks, setTasks] = useState<AgentQueueTask[]>([]);
    const [runs, setRuns] = useState<AgentRun[]>([]);
    const [goalRuns, setGoalRuns] = useState<GoalRun[]>([]);
    const [heartbeatItems, setHeartbeatItems] = useState<HeartbeatItem[]>([]);
    const [newTaskTitle, setNewTaskTitle] = useState("");
    const [newTaskDescription, setNewTaskDescription] = useState("");
    const [newTaskCommand, setNewTaskCommand] = useState("");
    const [newTaskSessionId, setNewTaskSessionId] = useState("");
    const [newTaskDependencies, setNewTaskDependencies] = useState("");
    const [newGoalTitle, setNewGoalTitle] = useState("");
    const [newGoalPrompt, setNewGoalPrompt] = useState("");
    const [newGoalSessionId, setNewGoalSessionId] = useState("");
    const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
    const [selectedGoalRunId, setSelectedGoalRunId] = useState<string | null>(null);
    const [goalActionId, setGoalActionId] = useState<string | null>(null);
    const [goalStartError, setGoalStartError] = useState<string | null>(null);
    const [historyFailureQuery, setHistoryFailureQuery] = useState("");
    const [historyMinReplans, setHistoryMinReplans] = useState(0);
    const [historyMinChildTasks, setHistoryMinChildTasks] = useState(0);
    const [historyMinApprovals, setHistoryMinApprovals] = useState(0);
    const [historyMinDurationMinutes, setHistoryMinDurationMinutes] = useState(0);

    const amux = getBridge();
    const goalRunsSupported = goalRunSupportAvailable();
    const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
    const createThread = useAgentStore((state) => state.createThread);
    const addMessage = useAgentStore((state) => state.addMessage);
    const setActiveThread = useAgentStore((state) => state.setActiveThread);
    const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
    const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
    const threads = useAgentStore((state) => state.threads);

    const refreshTasks = useCallback(async () => {
        const result = await fetchAgentTasks();
        setTasks(result);
        setSelectedTaskId((current) => current ?? result[0]?.id ?? null);
    }, []);

    const refreshRuns = useCallback(async () => {
        const result = await fetchAgentRuns();
        setRuns(result);
    }, []);

    const refreshGoalRuns = useCallback(async () => {
        if (!goalRunsSupported) {
            setGoalRuns([]);
            return;
        }

        const result = await fetchGoalRuns();
        setGoalRuns(result);
        setSelectedGoalRunId((current) => current ?? result[0]?.id ?? null);
    }, [goalRunsSupported]);

    const refreshHeartbeat = useCallback(async () => {
        if (!amux?.agentHeartbeatGetItems) return;
        try {
            const result = await amux.agentHeartbeatGetItems();
            setHeartbeatItems(Array.isArray(result) ? result as HeartbeatItem[] : []);
        } catch {
            /* silent */
        }
    }, [amux]);

    useEffect(() => {
        void refreshTasks();
        void refreshRuns();
        void refreshGoalRuns();
        void refreshHeartbeat();
        const interval = setInterval(() => {
            void refreshTasks();
            void refreshRuns();
            void refreshGoalRuns();
            void refreshHeartbeat();
        }, 5000);
        return () => clearInterval(interval);
    }, [refreshGoalRuns, refreshHeartbeat, refreshRuns, refreshTasks]);

    useEffect(() => {
        if (!amux?.onAgentEvent) return;
        const unsubscribe = amux.onAgentEvent((event: any) => {
            if (!event?.type) return;
            if (event.type === "goal_run_update" || event.type === "goal_run_created" || event.type === "todo_update") {
                void refreshGoalRuns();
            }
            if (event.type === "task_update") {
                void refreshTasks();
                void refreshRuns();
            }
        });
        return () => unsubscribe?.();
    }, [amux, refreshGoalRuns, refreshRuns, refreshTasks]);

    const addTask = async () => {
        if (!newTaskTitle.trim() || !amux?.agentAddTask) return;
        await amux.agentAddTask({
            title: newTaskTitle.trim(),
            description: (newTaskDescription || newTaskTitle).trim(),
            priority: "normal",
            command: newTaskCommand.trim() || null,
            sessionId: newTaskSessionId.trim() || null,
            dependencies: newTaskDependencies
                .split(",")
                .map((value) => value.trim())
                .filter(Boolean),
        });
        setNewTaskTitle("");
        setNewTaskDescription("");
        setNewTaskCommand("");
        setNewTaskSessionId("");
        setNewTaskDependencies("");
        void refreshTasks();
        void refreshRuns();
    };

    const addGoalRun = async () => {
        if (!goalRunsSupported || !newGoalPrompt.trim()) return;

        setGoalStartError(null);
        const provision = newGoalSessionId.trim()
            ? null
            : await provisionAgentWorkspaceTerminals({
                title: newGoalTitle.trim() || newGoalPrompt.trim(),
                cwd: activeWorkspace?.cwd ?? null,
            });
        const goalRun = await startGoalRun({
            goal: newGoalPrompt.trim(),
            title: newGoalTitle.trim() || null,
            sessionId: newGoalSessionId.trim() || provision?.coordinatorSessionId || null,
            priority: "normal",
        });

        if (!goalRun) {
            setGoalStartError("Goal runner backend is not available yet.");
            return;
        }

        setNewGoalTitle("");
        setNewGoalPrompt("");
        setNewGoalSessionId("");
        setSelectedGoalRunId(goalRun.id);
        void refreshGoalRuns();
    };

    const changeGoalRunState = async (
        goalRunId: string,
        action: "pause" | "resume" | "cancel" | "retry_step" | "rerun_from_step",
        stepIndex?: number,
    ) => {
        setGoalActionId(goalRunId);
        try {
            await controlGoalRun(goalRunId, action, stepIndex ?? null);
            await refreshGoalRuns();
        } finally {
            setGoalActionId(null);
        }
    };

    const cancelTask = async (taskId: string) => {
        if (!amux?.agentCancelTask) return;
        await amux.agentCancelTask(taskId);
        void refreshTasks();
        void refreshRuns();
    };

    const openTaskThread = useCallback(async (task: ThreadTarget) => {
        if (!task.thread_id || !amux?.agentGetThread) {
            return;
        }

        const existingThread = threads.find((entry) => entry.daemonThreadId === task.thread_id);
        if (existingThread) {
            setActiveThread(existingThread.id);
            onOpenThreadView?.();
            return;
        }

        const remoteThread = await amux.agentGetThread(task.thread_id) as RemoteAgentThread | null;
        if (!remoteThread) {
            return;
        }

        const location = findTaskWorkspaceLocation(useWorkspaceStore.getState().workspaces, task.session_id);
        const localThreadId = createThread({
            workspaceId: location?.workspaceId ?? null,
            surfaceId: location?.surfaceId ?? null,
            paneId: location?.paneId ?? null,
            title: remoteThread.title || task.title,
        });
        setThreadDaemonId(localThreadId, remoteThread.id);

        for (const message of remoteThread.messages ?? []) {
            addMessage(localThreadId, {
                role: message.role,
                content: message.content ?? "",
                provider: undefined,
                model: undefined,
                toolCalls: Array.isArray(message.tool_calls) ? message.tool_calls as any : undefined,
                toolName: message.tool_name ?? undefined,
                toolCallId: message.tool_call_id ?? undefined,
                toolArguments: message.tool_arguments ?? undefined,
                toolStatus: message.tool_status === "requested" || message.tool_status === "executing" || message.tool_status === "done" || message.tool_status === "error"
                    ? message.tool_status
                    : undefined,
                inputTokens: message.input_tokens ?? 0,
                outputTokens: message.output_tokens ?? 0,
                totalTokens: (message.input_tokens ?? 0) + (message.output_tokens ?? 0),
                reasoning: message.reasoning ?? undefined,
                isCompactionSummary: false,
                isStreaming: false,
            });
        }

        const todos = await fetchThreadTodos(remoteThread.id).catch(() => []);
        setThreadTodos(localThreadId, todos);
        setActiveThread(localThreadId);
        onOpenThreadView?.();
    }, [addMessage, amux, createThread, onOpenThreadView, setActiveThread, setThreadDaemonId, setThreadTodos, threads]);

    const topLevelTasks = useMemo(
        () => tasks.filter((task) => !task.parent_task_id),
        [tasks],
    );
    const subagentRunsByParent = useMemo(() => {
        const grouped = new Map<string, AgentRun[]>();
        for (const run of runs) {
            if (run.kind !== "subagent") {
                continue;
            }
            const parentKey = run.parent_run_id
                ? `task:${run.parent_run_id}`
                : run.parent_thread_id
                    ? `thread:${run.parent_thread_id}`
                    : null;
            if (!parentKey) {
                continue;
            }
            const bucket = grouped.get(parentKey) ?? [];
            bucket.push(run);
            grouped.set(parentKey, bucket);
        }
        return grouped;
    }, [runs]);
    const activeTasks = topLevelTasks.filter(isTaskActive);
    const completedTasks = topLevelTasks.filter((task) => !isTaskActive(task));
    const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? topLevelTasks[0] ?? tasks[0] ?? null;
    const selectedTaskSubagents = useMemo(() => {
        if (!selectedTask) {
            return [] as AgentRun[];
        }
        const directChildren = subagentRunsByParent.get(`task:${selectedTask.id}`) ?? [];
        return directChildren.slice().sort((a, b) => b.created_at - a.created_at);
    }, [selectedTask, subagentRunsByParent]);

    const activeGoalRuns = useMemo(() => goalRuns.filter(isGoalRunActive), [goalRuns]);
    const historicalGoalRuns = useMemo(() => goalRuns.filter((goalRun) => !isGoalRunActive(goalRun)), [goalRuns]);
    const completedGoalRuns = useMemo(() => {
        const failureQuery = historyFailureQuery.trim().toLowerCase();
        return historicalGoalRuns.filter((goalRun) => {
            const durationMinutes = typeof goalRun.duration_ms === "number" ? goalRun.duration_ms / 60000 : 0;
            const failureText = `${goalRun.failure_cause ?? ""} ${goalRun.last_error ?? ""} ${goalRun.error ?? ""}`.toLowerCase();

            if (goalRun.replan_count < historyMinReplans) {
                return false;
            }
            if (goalRunChildTaskCount(goalRun) < historyMinChildTasks) {
                return false;
            }
            if ((goalRun.approval_count ?? 0) < historyMinApprovals) {
                return false;
            }
            if (durationMinutes < historyMinDurationMinutes) {
                return false;
            }
            if (failureQuery && !failureText.includes(failureQuery)) {
                return false;
            }
            return true;
        });
    }, [historicalGoalRuns, historyFailureQuery, historyMinApprovals, historyMinChildTasks, historyMinDurationMinutes, historyMinReplans]);
    const selectedGoalRun = goalRuns.find((goalRun) => goalRun.id === selectedGoalRunId) ?? goalRuns[0] ?? null;

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <SectionTitle title="Goal Runners" subtitle="Durable autonomous jobs that plan, execute, reflect, and learn over time" />

            {goalRunsSupported ? (
                <>
                    <div style={{ marginBottom: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                        <textarea
                            placeholder="Define a long-running goal..."
                            value={newGoalPrompt}
                            onChange={(event) => setNewGoalPrompt(event.target.value)}
                            rows={3}
                            style={inputBlockStyle}
                        />
                        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                            <input
                                type="text"
                                placeholder="Goal title (optional)..."
                                value={newGoalTitle}
                                onChange={(event) => setNewGoalTitle(event.target.value)}
                                style={inputRowStyle}
                            />
                            <input
                                type="text"
                                placeholder="Target session ID (optional)..."
                                value={newGoalSessionId}
                                onChange={(event) => setNewGoalSessionId(event.target.value)}
                                style={inputRowStyle}
                            />
                        </div>
                        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                                Goal runners sit above the task queue and can spawn child tasks as they replan.
                            </div>
                            <ActionButton onClick={() => void addGoalRun()} disabled={!newGoalPrompt.trim()}>
                                Start Goal Run
                            </ActionButton>
                        </div>
                        {goalStartError && (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>
                                {goalStartError}
                            </div>
                        )}
                    </div>

                    {activeGoalRuns.length > 0 && (
                        <div style={{ marginBottom: "var(--space-4)" }}>
                            <div style={sectionLabelStyle}>Active ({activeGoalRuns.length})</div>
                            {activeGoalRuns.map((goalRun) => (
                                <GoalRunCard
                                    key={goalRun.id}
                                    goalRun={goalRun}
                                    selected={goalRun.id === selectedGoalRun?.id}
                                    busy={goalActionId === goalRun.id}
                                    onSelect={() => setSelectedGoalRunId(goalRun.id)}
                                    onPause={goalRun.status === "running" ? () => void changeGoalRunState(goalRun.id, "pause") : undefined}
                                    onResume={goalRun.status === "paused" ? () => void changeGoalRunState(goalRun.id, "resume") : undefined}
                                    onCancel={goalRun.status !== "completed" && goalRun.status !== "failed" && goalRun.status !== "cancelled"
                                        ? () => void changeGoalRunState(goalRun.id, "cancel")
                                        : undefined}
                                />
                            ))}
                        </div>
                    )}

                    {historicalGoalRuns.length > 0 && (
                        <div style={{ marginBottom: "var(--space-4)" }}>
                            <div style={sectionLabelStyle}>History ({completedGoalRuns.length} shown of {historicalGoalRuns.length})</div>
                            <div
                                style={{
                                    display: "grid",
                                    gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
                                    gap: "var(--space-2)",
                                    marginBottom: "var(--space-3)",
                                }}
                            >
                                <input
                                    type="text"
                                    placeholder="Failure cause filter..."
                                    value={historyFailureQuery}
                                    onChange={(event) => setHistoryFailureQuery(event.target.value)}
                                    style={inputRowStyle}
                                />
                                <input
                                    type="number"
                                    min={0}
                                    placeholder="Min replans"
                                    value={historyMinReplans}
                                    onChange={(event) => setHistoryMinReplans(Number(event.target.value) || 0)}
                                    style={inputRowStyle}
                                />
                                <input
                                    type="number"
                                    min={0}
                                    placeholder="Min child tasks"
                                    value={historyMinChildTasks}
                                    onChange={(event) => setHistoryMinChildTasks(Number(event.target.value) || 0)}
                                    style={inputRowStyle}
                                />
                                <input
                                    type="number"
                                    min={0}
                                    placeholder="Min approvals"
                                    value={historyMinApprovals}
                                    onChange={(event) => setHistoryMinApprovals(Number(event.target.value) || 0)}
                                    style={inputRowStyle}
                                />
                                <input
                                    type="number"
                                    min={0}
                                    placeholder="Min duration (min)"
                                    value={historyMinDurationMinutes}
                                    onChange={(event) => setHistoryMinDurationMinutes(Number(event.target.value) || 0)}
                                    style={inputRowStyle}
                                />
                            </div>
                            {completedGoalRuns.length > 0 ? completedGoalRuns.slice(0, 12).map((goalRun) => (
                                <GoalRunCard
                                    key={goalRun.id}
                                    goalRun={goalRun}
                                    selected={goalRun.id === selectedGoalRun?.id}
                                    busy={false}
                                    onSelect={() => setSelectedGoalRunId(goalRun.id)}
                                />
                            )) : (
                                <EmptyPanel message="No historical goal runs match the current filters." />
                            )}
                        </div>
                    )}

                    {goalRuns.length === 0 && (
                        <EmptyPanel message="No goal runs yet. Start a durable goal to let the daemon plan, execute, and reflect over time." />
                    )}

                    {selectedGoalRun && (
                        <div style={{ marginBottom: "var(--space-5)" }}>
                            <SectionTitle title="Goal Run Detail" subtitle="Current plan, state, and learning output for the selected goal run" />
                            <GoalRunDetail
                                goalRun={selectedGoalRun}
                                busy={goalActionId === selectedGoalRun.id}
                                onRetryStep={(stepIndex) => void changeGoalRunState(selectedGoalRun.id, "retry_step", stepIndex)}
                                onRerunFromStep={(stepIndex) => void changeGoalRunState(selectedGoalRun.id, "rerun_from_step", stepIndex)}
                            />
                        </div>
                    )}
                </>
            ) : (
                <EmptyPanel message="Goal-runner controls will appear here when the backend exposes goal-run IPC methods." />
            )}

            <SectionTitle title="Task Queue" subtitle="Autonomous task execution by daemon agent" />

            <div style={{ marginBottom: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                <input
                    type="text"
                    placeholder="Task title..."
                    value={newTaskTitle}
                    onChange={(e) => setNewTaskTitle(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && addTask()}
                    style={inputRowStyle}
                />
                {newTaskTitle && (
                    <textarea
                        placeholder="Description (optional)..."
                        value={newTaskDescription}
                        onChange={(e) => setNewTaskDescription(e.target.value)}
                        rows={2}
                        style={inputBlockStyle}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Preferred command or entrypoint (optional)..."
                        value={newTaskCommand}
                        onChange={(e) => setNewTaskCommand(e.target.value)}
                        style={inputRowStyle}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Target session ID (optional)..."
                        value={newTaskSessionId}
                        onChange={(e) => setNewTaskSessionId(e.target.value)}
                        style={inputRowStyle}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Dependencies: task IDs, comma-separated (optional)..."
                        value={newTaskDependencies}
                        onChange={(e) => setNewTaskDependencies(e.target.value)}
                        style={inputRowStyle}
                    />
                )}
                {newTaskTitle && (
                    <ActionButton onClick={addTask}>Add Task</ActionButton>
                )}
            </div>

            {activeTasks.length > 0 && (
                <div style={{ marginBottom: "var(--space-4)" }}>
                    <div style={sectionLabelStyle}>Active ({activeTasks.length})</div>
                    {activeTasks.map((task) => (
                        <TaskCard key={task.id} task={task} selected={task.id === selectedTask?.id} onSelect={() => setSelectedTaskId(task.id)} onCancel={() => cancelTask(task.id)} />
                    ))}
                </div>
            )}

            {completedTasks.length > 0 && (
                <div style={{ marginBottom: "var(--space-4)" }}>
                    <div style={sectionLabelStyle}>History ({completedTasks.length})</div>
                    {completedTasks.slice(0, 20).map((task) => (
                        <TaskCard key={task.id} task={task} selected={task.id === selectedTask?.id} onSelect={() => setSelectedTaskId(task.id)} />
                    ))}
                </div>
            )}

            {tasks.length === 0 && (
                <div style={{ textAlign: "center", padding: "var(--space-6)", color: "var(--text-muted)", fontSize: "var(--text-sm)" }}>
                    No tasks yet. Add a task above or let a goal runner enqueue child work.
                </div>
            )}

            {selectedTask && (
                <div style={{ marginBottom: "var(--space-5)" }}>
                    <SectionTitle title="Post-Mortem" subtitle="Latest trajectory for the selected task" />
                    <TaskPostMortem
                        task={selectedTask}
                        subagents={selectedTaskSubagents}
                        onSelectTask={setSelectedTaskId}
                        onOpenTaskThread={(task) => void openTaskThread(task)}
                    />
                </div>
            )}

            <SectionTitle title="Heartbeat" subtitle="Periodic health checks run by the daemon" />

            {heartbeatItems.length > 0 ? (
                heartbeatItems.map((item) => (
                    <HeartbeatCard key={item.id} item={item} />
                ))
            ) : (
                <div style={{ textAlign: "center", padding: "var(--space-4)", color: "var(--text-muted)", fontSize: "var(--text-xs)" }}>
                    No heartbeat checks configured. Edit ~/.tamux/agent/heartbeat.json to add checks.
                </div>
            )}
        </div>
    );
}

function GoalRunCard({
    goalRun,
    selected,
    busy,
    onSelect,
    onPause,
    onResume,
    onCancel,
}: {
    goalRun: GoalRun;
    selected: boolean;
    busy: boolean;
    onSelect: () => void;
    onPause?: () => void;
    onResume?: () => void;
    onCancel?: () => void;
}) {
    const statusColor = goalRunStatusColor(goalRun.status);
    const active = isGoalRunActive(goalRun);

    return (
        <div
            role="button"
            tabIndex={0}
            onClick={onSelect}
            onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelect();
                }
            }}
            style={{
                width: "100%",
                textAlign: "left",
                padding: "var(--space-3)",
                borderRadius: "var(--radius-md)",
                border: selected ? `1px solid ${statusColor}` : "1px solid var(--border)",
                background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                marginBottom: "var(--space-2)",
                cursor: "pointer",
            }}
        >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
                        <div style={{ fontSize: "var(--text-sm)", fontWeight: 600, color: "var(--text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            {goalRun.title}
                        </div>
                        <span style={{ padding: "2px 8px", borderRadius: 999, fontSize: "var(--text-xs)", border: "1px solid var(--glass-border)", color: statusColor }}>
                            {formatGoalRunStatus(goalRun.status)}
                        </span>
                    </div>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4 }}>
                        {summarizeGoalRunStep(goalRun)}
                        <span style={{ marginLeft: "var(--space-2)" }}>{formatTaskTimestamp(goalRun.created_at)}</span>
                        <span style={{ marginLeft: "var(--space-2)" }}>{goalRunChildTaskCount(goalRun)} child task{goalRunChildTaskCount(goalRun) === 1 ? "" : "s"}</span>
                        <span style={{ marginLeft: "var(--space-2)" }}>{goalRun.replan_count} replan{goalRun.replan_count === 1 ? "" : "s"}</span>
                        <span style={{ marginLeft: "var(--space-2)" }}>{goalRun.approval_count ?? 0} approval{(goalRun.approval_count ?? 0) === 1 ? "" : "s"}</span>
                        <span style={{ marginLeft: "var(--space-2)" }}>{formatGoalRunDuration(goalRun.duration_ms)}</span>
                    </div>
                </div>
                {active && (
                    <div style={{ display: "flex", gap: "var(--space-1)", flexShrink: 0 }}>
                        {onPause && (
                            <button type="button" onClick={(event) => { event.stopPropagation(); onPause(); }} style={{ ...iconButtonStyle, fontSize: 11 }} disabled={busy}>
                                Pause
                            </button>
                        )}
                        {onResume && (
                            <button type="button" onClick={(event) => { event.stopPropagation(); onResume(); }} style={{ ...iconButtonStyle, fontSize: 11 }} disabled={busy}>
                                Resume
                            </button>
                        )}
                        {onCancel && (
                            <button type="button" onClick={(event) => { event.stopPropagation(); onCancel(); }} style={{ ...iconButtonStyle, fontSize: 11 }} disabled={busy}>
                                Cancel
                            </button>
                        )}
                    </div>
                )}
            </div>
            {goalRun.plan_summary && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
                    {goalRun.plan_summary}
                </div>
            )}
            {(goalRun.failure_cause || goalRun.last_error || goalRun.error) && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
                    {goalRun.failure_cause ?? goalRun.last_error ?? goalRun.error}
                </div>
            )}
        </div>
    );
}

function GoalRunDetail({
    goalRun,
    busy,
    onRetryStep,
    onRerunFromStep,
}: {
    goalRun: GoalRun;
    busy: boolean;
    onRetryStep: (stepIndex: number) => void;
    onRerunFromStep: (stepIndex: number) => void;
}) {
    const currentStep = typeof goalRun.current_step_index === "number" && goalRun.steps?.length
        ? goalRun.steps[goalRun.current_step_index] ?? null
        : null;
    const latestTodos = latestGoalRunTodoSnapshot(goalRun);

    return (
        <div
            style={{
                padding: "var(--space-3)",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--bg-secondary)",
                display: "flex",
                flexDirection: "column",
                gap: "var(--space-3)",
            }}
        >
            <div>
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{goalRun.title}</div>
                <div style={{ fontSize: "var(--text-xs)", color: goalRunStatusColor(goalRun.status), marginTop: 4 }}>
                    {formatGoalRunStatus(goalRun.status)}
                </div>
            </div>

            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", whiteSpace: "pre-wrap" }}>
                {goalRun.goal}
            </div>

            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "var(--space-2)" }}>
                <DetailCard label="Current Step" value={currentStep?.title || goalRun.current_step_title || "-"} />
                <DetailCard label="Step Kind" value={currentStep?.kind || goalRun.current_step_kind || "-"} />
                <DetailCard label="Thread" value={goalRun.thread_id || "-"} />
                <DetailCard label="Session" value={goalRun.session_id || currentStep?.session_id || "-"} />
                <DetailCard label="Approval" value={goalRun.awaiting_approval_id || "-"} />
                <DetailCard label="Skill" value={goalRun.generated_skill_path || "-"} />
                <DetailCard label="Duration" value={formatGoalRunDuration(goalRun.duration_ms)} />
                <DetailCard label="Replans" value={String(goalRun.replan_count)} />
                <DetailCard label="Child Tasks" value={String(goalRunChildTaskCount(goalRun))} />
                <DetailCard label="Approvals" value={String(goalRun.approval_count ?? 0)} />
            </div>

            {goalRun.plan_summary && (
                <div>
                    <div style={detailLabelStyle}>Plan</div>
                    <div style={detailBodyStyle}>{goalRun.plan_summary}</div>
                </div>
            )}

            {goalRun.reflection_summary && (
                <div>
                    <div style={detailLabelStyle}>Reflection</div>
                    <div style={detailBodyStyle}>{goalRun.reflection_summary}</div>
                </div>
            )}

            {goalRun.steps && goalRun.steps.length > 0 && (
                <div>
                    <div style={detailLabelStyle}>Plan Steps</div>
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                        {goalRun.steps.map((step, index) => {
                            const active = goalRun.current_step_index === index;
                            return (
                                <div
                                    key={step.id}
                                    style={{
                                        padding: "var(--space-2)",
                                        borderRadius: "var(--radius-sm)",
                                        background: active ? "var(--mission-soft)" : "var(--bg-tertiary)",
                                        border: active ? "1px solid var(--mission-border)" : "1px solid transparent",
                                    }}
                                >
                                    <div style={{ fontSize: "var(--text-xs)", color: active ? "var(--mission)" : "var(--text-muted)" }}>
                                        Step {index + 1} · {step.kind}{step.status ? ` · ${step.status}` : ""}
                                    </div>
                                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{step.title}</div>
                                    {step.success_condition && (
                                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                                            Success: {step.success_condition}
                                        </div>
                                    )}
                                    <div style={{ display: "flex", gap: "var(--space-2)", marginTop: "var(--space-2)", flexWrap: "wrap" }}>
                                        {(step.status === "failed" || (active && goalRun.status === "failed")) && (
                                            <button
                                                type="button"
                                                onClick={() => onRetryStep(index)}
                                                style={{ ...iconButtonStyle, fontSize: 11 }}
                                                disabled={busy}
                                            >
                                                Retry Step
                                            </button>
                                        )}
                                        <button
                                            type="button"
                                            onClick={() => onRerunFromStep(index)}
                                            style={{ ...iconButtonStyle, fontSize: 11 }}
                                            disabled={busy}
                                        >
                                            Rerun From Here
                                        </button>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                </div>
            )}

            {latestTodos.length > 0 && (
                <div>
                    <div style={detailLabelStyle}>Current Todo</div>
                    <TodoSnapshotList items={latestTodos} />
                </div>
            )}

            {goalRun.events && goalRun.events.length > 0 && (
                <div>
                    <div style={detailLabelStyle}>Timeline</div>
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                        {goalRun.events
                            .slice()
                            .sort((a, b) => b.timestamp - a.timestamp)
                            .map((event) => (
                                <GoalRunTimelineEvent key={event.id} event={event} />
                            ))}
                    </div>
                </div>
            )}

            {goalRun.memory_updates && goalRun.memory_updates.length > 0 && (
                <div>
                    <div style={detailLabelStyle}>Memory Updates</div>
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                        {goalRun.memory_updates.map((entry, index) => (
                            <div key={`${goalRun.id}-memory-${index}`} style={detailBodyStyle}>{entry}</div>
                        ))}
                    </div>
                </div>
            )}

            {(goalRun.result || goalRun.failure_cause || goalRun.last_error || goalRun.error) && (
                <div>
                    <div style={detailLabelStyle}>{goalRun.result ? "Outcome" : "Error"}</div>
                    <div style={detailBodyStyle}>{goalRun.result || goalRun.failure_cause || goalRun.last_error || goalRun.error}</div>
                </div>
            )}
        </div>
    );
}

function GoalRunTimelineEvent({ event }: { event: GoalRunEvent }) {
    return (
        <div
            style={{
                padding: "var(--space-2)",
                borderRadius: "var(--radius-sm)",
                background: "var(--bg-tertiary)",
                display: "flex",
                flexDirection: "column",
                gap: "var(--space-1)",
            }}
        >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "capitalize" }}>
                    {event.phase.replace(/_/g, " ")}
                    {typeof event.step_index === "number" ? ` · step ${event.step_index + 1}` : ""}
                </div>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    {formatTaskTimestamp(event.timestamp)}
                </div>
            </div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>
                {event.message}
            </div>
            {event.details && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>
                    {event.details}
                </div>
            )}
            {event.todo_snapshot.length > 0 && <TodoSnapshotList items={event.todo_snapshot} />}
        </div>
    );
}

function TodoSnapshotList({ items }: { items: TodoItem[] }) {
    return (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)", marginTop: 4 }}>
            {items
                .slice()
                .sort((a, b) => a.position - b.position)
                .map((item) => (
                    <div
                        key={item.id}
                        style={{
                            display: "flex",
                            alignItems: "center",
                            gap: "var(--space-2)",
                            padding: "6px 8px",
                            borderRadius: "var(--radius-sm)",
                            background: "rgba(255,255,255,0.03)",
                        }}
                    >
                        <span
                            style={{
                                width: 8,
                                height: 8,
                                borderRadius: "50%",
                                background: todoStatusColor(item.status),
                                flexShrink: 0,
                            }}
                        />
                        <span style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", flex: 1 }}>
                            {item.content}
                        </span>
                        <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "capitalize" }}>
                            {item.status.replace(/_/g, " ")}
                        </span>
                    </div>
                ))}
        </div>
    );
}

function DetailCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
            <div style={detailLabelStyle}>{label}</div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2, wordBreak: "break-word" }}>{value}</div>
        </div>
    );
}

function todoStatusColor(status: TodoItem["status"]): string {
    switch (status) {
        case "in_progress":
            return "var(--accent)";
        case "completed":
            return "var(--success)";
        case "blocked":
            return "var(--warning)";
        default:
            return "var(--text-muted)";
    }
}

function TaskCard({
    task,
    selected,
    onSelect,
    onCancel,
}: {
    task: AgentQueueTask;
    selected: boolean;
    onSelect: () => void;
    onCancel?: () => void;
}) {
    const statusColor = taskStatusColor(task.status);
    const isActive = isTaskActive(task);

    return (
        <div
            role="button"
            tabIndex={0}
            onClick={onSelect}
            onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelect();
                }
            }}
            style={{
                width: "100%",
                textAlign: "left",
                padding: "var(--space-3)",
                borderRadius: "var(--radius-md)",
                border: selected ? `1px solid ${statusColor}` : "1px solid var(--border)",
                background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                marginBottom: "var(--space-2)",
                cursor: "pointer",
            }}
        >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 500, color: "var(--text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {task.title}
                    </div>
                    {task.goal_run_title && (
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            Goal: {task.goal_run_title}
                        </div>
                    )}
                    {task.source === "subagent" && (
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            Subagent · runtime {task.runtime ?? "daemon"}
                        </div>
                    )}
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
                        <span style={{ color: statusColor, fontWeight: 600 }}>{formatTaskStatus(task)}</span>
                        {task.status === "in_progress" && task.progress > 0 && (
                            <span> {task.progress}%</span>
                        )}
                        <span style={{ marginLeft: "var(--space-2)" }}>
                            {formatTaskTimestamp(task.created_at)}
                        </span>
                        {typeof task.retry_count === "number" && typeof task.max_retries === "number" && (
                            <span style={{ marginLeft: "var(--space-2)" }}>
                                retry {task.retry_count}/{task.max_retries === 0 ? "∞" : task.max_retries}
                            </span>
                        )}
                    </div>
                </div>
                {isActive && onCancel && (
                    <button type="button" onClick={(event) => { event.stopPropagation(); onCancel(); }} style={{ ...iconButtonStyle, fontSize: 11 }} title="Cancel task">
                        Cancel
                    </button>
                )}
            </div>
            {(task.blocked_reason || task.error) && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
                    {task.blocked_reason ?? task.error}
                </div>
            )}
            {task.command && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
                    {task.command}
                </div>
            )}
        </div>
    );
}

function TaskPostMortem({
    task,
    subagents,
    onSelectTask,
    onOpenTaskThread,
}: {
    task: AgentQueueTask;
    subagents: AgentRun[];
    onSelectTask: (taskId: string) => void;
    onOpenTaskThread: (task: ThreadTarget) => void;
}) {
    const workspaces = useWorkspaceStore((state) => state.workspaces);
    const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
    const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
    const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
    const focusCanvasPanel = useWorkspaceStore((state) => state.focusCanvasPanel);
    const logs = [...(task.logs ?? [])].slice(-8).reverse();
    const location = useMemo(
        () => findTaskWorkspaceLocation(workspaces, task.session_id),
        [task.session_id, workspaces],
    );
    const openTaskSession = useCallback(() => {
        if (!location) {
            return;
        }
        setActiveWorkspace(location.workspaceId);
        setActiveSurface(location.surfaceId);
        focusCanvasPanel(location.paneId, { storePreviousView: true });
        setActivePaneId(location.paneId);
    }, [focusCanvasPanel, location, setActivePaneId, setActiveSurface, setActiveWorkspace]);

    return (
        <div
            style={{
                padding: "var(--space-3)",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--bg-secondary)",
            }}
        >
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{task.title}</div>
            {task.goal_run_title && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                    Goal: {task.goal_run_title}
                    {task.goal_step_title && task.goal_step_title !== task.title ? ` · Step: ${task.goal_step_title}` : ""}
                </div>
            )}
            <div style={{ fontSize: "var(--text-xs)", color: taskStatusColor(task.status), marginTop: 4 }}>
                {formatTaskStatus(task)}
            </div>
            {task.command && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
                    Command: {task.command}
                </div>
            )}
            {task.session_id && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                    Session: {task.session_id}
                </div>
            )}
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                Runtime: {task.runtime ?? "daemon"}
            </div>
            {location && (
                <div style={{ marginTop: "var(--space-2)", display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        Workspace: {location.workspaceName} · Surface: {location.surfaceName}
                        {location.cwd ? ` · ${shortenHomePath(location.cwd)}` : ""}
                    </div>
                    <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                        {task.thread_id && <ActionButton onClick={() => onOpenTaskThread(task)}>Open Chat</ActionButton>}
                        <ActionButton onClick={openTaskSession}>Open Session</ActionButton>
                    </div>
                </div>
            )}
            {!location && task.thread_id && (
                <div style={{ marginTop: "var(--space-2)", display: "flex", justifyContent: "flex-end" }}>
                    <ActionButton onClick={() => onOpenTaskThread(task)}>Open Chat</ActionButton>
                </div>
            )}
            {task.dependencies && task.dependencies.length > 0 && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                    Depends on: {task.dependencies.join(", ")}
                </div>
            )}
            {task.parent_task_id && (
                <div style={{ marginTop: 4, display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                        Parent task: {task.parent_task_id}
                    </div>
                    <ActionButton onClick={() => onSelectTask(task.parent_task_id!)}>Back To Parent</ActionButton>
                </div>
            )}
            {task.last_error && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
                    {task.last_error}
                </div>
            )}
            <TaskCodePreview task={task} location={location} />
            <div style={{ marginTop: "var(--space-3)" }}>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: "var(--space-2)" }}>
                    Subagents ({subagents.length})
                </div>
                {subagents.length > 0 ? (
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                        {subagents.map((subagent) => (
                            <div
                                key={subagent.id}
                                style={{
                                    padding: "var(--space-2)",
                                    borderRadius: "var(--radius-sm)",
                                    background: "var(--bg-tertiary)",
                                    border: `1px solid ${runStatusColor(subagent.status)}`,
                                }}
                            >
                                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                    <div>
                                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{subagent.title}</div>
                                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2 }}>
                                            {formatRunStatus(subagent)} · runtime {subagent.runtime ?? "daemon"}
                                            {subagent.classification ? ` · ${subagent.classification}` : ""}
                                            {subagent.session_id ? ` · session ${subagent.session_id}` : ""}
                                        </div>
                                    </div>
                                    <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                        {subagent.thread_id && (
                                            <ActionButton onClick={() => onOpenTaskThread(subagent)}>Open Chat</ActionButton>
                                        )}
                                        <ActionButton onClick={() => onSelectTask(subagent.id)}>Inspect</ActionButton>
                                    </div>
                                </div>
                                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-2)" }}>
                                    {subagent.description}
                                </div>
                            </div>
                        ))}
                    </div>
                ) : (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                        No child subagents have been spawned for this task.
                    </div>
                )}
            </div>
            <div style={{ marginTop: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                {logs.length > 0 ? logs.map((log) => (
                    <div key={log.id} style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
                        <div style={{ fontSize: "var(--text-xs)", color: log.level === "error" ? "var(--danger)" : log.level === "warn" ? "var(--warning)" : "var(--text-muted)" }}>
                            {log.phase} · attempt {log.attempt || 0} · {formatTaskTimestamp(log.timestamp)}
                        </div>
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{log.message}</div>
                        {log.details && (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>{log.details}</div>
                        )}
                    </div>
                )) : (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>No task logs recorded yet.</div>
                )}
            </div>
        </div>
    );
}

function TaskCodePreview({
    task,
    location,
}: {
    task: AgentQueueTask;
    location: TaskWorkspaceLocation | null;
}) {
    const [context, setContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });
    const [selectedPath, setSelectedPath] = useState<string | null>(null);
    const [previewText, setPreviewText] = useState("");
    const [loadingEntries, setLoadingEntries] = useState(false);
    const [loadingDiff, setLoadingDiff] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const codingTask = taskLooksLikeCoding(task);
    const threadId = task.thread_id || null;
    const bridge = getBridge();
    const selectedEntry = useMemo(
        () => context.entries.find((entry) => entry.path === selectedPath) ?? null,
        [context.entries, selectedPath],
    );

    useEffect(() => {
        if (!threadId) {
            setContext({ threadId: "", entries: [] });
            setSelectedPath(null);
            setPreviewText("");
            setLoadingEntries(false);
            setLoadingDiff(false);
            setError(null);
            return;
        }

        let cancelled = false;
        setLoadingEntries(true);
        setError(null);

        void fetchThreadWorkContext(threadId)
            .then((nextContext) => {
                if (cancelled) {
                    return;
                }

                setContext(nextContext);
                setSelectedPath((current) => (
                    current && nextContext.entries.some((entry) => entry.path === current)
                        ? current
                        : nextContext.entries[0]?.path ?? null
                ));
            })
            .catch((reason: unknown) => {
                if (cancelled) {
                    return;
                }
                setContext({ threadId, entries: [] });
                setSelectedPath(null);
                setPreviewText("");
                setError(reason instanceof Error ? reason.message : String(reason));
            })
            .finally(() => {
                if (!cancelled) {
                    setLoadingEntries(false);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [task.id, threadId]);

    useEffect(() => {
        if (!threadId || !bridge?.onAgentEvent) {
            return;
        }
        return bridge.onAgentEvent((event: any) => {
            if (event?.type !== "work_context_update" || event?.thread_id !== threadId) {
                return;
            }
            void fetchThreadWorkContext(threadId).then((nextContext) => {
                setContext(nextContext);
                setSelectedPath((current) => (
                    current && nextContext.entries.some((entry) => entry.path === current)
                        ? current
                        : nextContext.entries[0]?.path ?? null
                ));
            });
        });
    }, [bridge, threadId]);

    useEffect(() => {
        if (!selectedEntry) {
            setPreviewText("");
            setLoadingDiff(false);
            return;
        }

        let cancelled = false;
        setLoadingDiff(true);
        setError(null);

        const previewPromise = selectedEntry.repoRoot
            ? fetchGitDiff(selectedEntry.repoRoot, selectedEntry.path)
            : fetchFilePreview(selectedEntry.path).then((preview) => preview?.content ?? "");

        void previewPromise
            .then((output) => {
                if (!cancelled) {
                    setPreviewText(output);
                }
            })
            .catch((reason: unknown) => {
                if (!cancelled) {
                    setPreviewText("");
                    setError(reason instanceof Error ? reason.message : String(reason));
                }
            })
            .finally(() => {
                if (!cancelled) {
                    setLoadingDiff(false);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [selectedEntry]);

    if (!threadId || (!codingTask && context.entries.length === 0 && !loadingEntries && !error)) {
        return null;
    }

    return (
        <div style={{ marginTop: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            <div style={detailLabelStyle}>Work Context</div>
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                Scope: {location?.cwd ? shortenHomePath(location.cwd) : "thread workspace"}
                <span style={{ marginLeft: "var(--space-2)" }}>
                    {loadingEntries ? "Refreshing..." : `${context.entries.length} file${context.entries.length === 1 ? "" : "s"} / artifact${context.entries.length === 1 ? "" : "s"}`}
                </span>
            </div>
            {error && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>
                    {error}
                </div>
            )}
            {context.entries.length > 0 ? (
                <div
                    style={{
                        display: "grid",
                        gridTemplateColumns: "minmax(220px, 280px) minmax(0, 1fr)",
                        gap: "var(--space-2)",
                    }}
                >
                    <div
                        style={{
                            display: "flex",
                            flexDirection: "column",
                            gap: "var(--space-1)",
                            maxHeight: 320,
                            overflow: "auto",
                        }}
                    >
                        {context.entries.map((entry) => {
                            const selected = entry.path === selectedPath;
                            return (
                                <button
                                    key={`${entry.source}:${entry.path}`}
                                    type="button"
                                    onClick={() => setSelectedPath(entry.path)}
                                    style={{
                                        textAlign: "left",
                                        padding: "var(--space-2)",
                                        borderRadius: "var(--radius-sm)",
                                        border: selected ? `1px solid ${workContextKindColor(entry)}` : "1px solid var(--border)",
                                        background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                                        cursor: "pointer",
                                    }}
                                >
                                    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", marginBottom: 4, flexWrap: "wrap" }}>
                                        <span style={{ fontSize: "var(--text-xs)", color: workContextKindColor(entry), fontWeight: 600 }}>
                                            {workContextKindLabel(entry)}
                                        </span>
                                        <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
                                            {entry.source}
                                        </span>
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-primary)", fontFamily: "var(--font-mono)", wordBreak: "break-word" }}>
                                        {entry.path}
                                    </div>
                                    {entry.previousPath && (
                                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4, wordBreak: "break-word" }}>
                                            from {entry.previousPath}
                                        </div>
                                    )}
                                </button>
                            );
                        })}
                    </div>
                    <div
                        style={{
                            minHeight: 220,
                            maxHeight: 320,
                            overflow: "auto",
                            padding: "var(--space-2)",
                            borderRadius: "var(--radius-sm)",
                            background: "var(--bg-tertiary)",
                            border: "1px solid var(--border)",
                        }}
                    >
                        {loadingDiff ? (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>Loading preview...</div>
                        ) : previewText.trim() ? (
                            <pre
                                style={{
                                    margin: 0,
                                    fontSize: "var(--text-xs)",
                                    lineHeight: 1.5,
                                    color: "var(--text-primary)",
                                    fontFamily: "var(--font-mono)",
                                    whiteSpace: "pre-wrap",
                                    wordBreak: "break-word",
                                }}
                            >
                                {previewText}
                            </pre>
                        ) : (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                                No preview available for the selected item.
                            </div>
                        )}
                    </div>
                </div>
            ) : (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
                    {loadingEntries ? "Refreshing work context..." : "No file or artifact activity detected for this task yet."}
                </div>
            )}
        </div>
    );
}

function HeartbeatCard({ item }: { item: HeartbeatItem }) {
    const resultColor = item.last_result ? (heartbeatColors[item.last_result] || "var(--text-muted)") : "var(--text-muted)";

    return (
        <div
            style={{
                padding: "var(--space-3)",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--bg-secondary)",
                marginBottom: "var(--space-2)",
                opacity: item.enabled ? 1 : 0.5,
            }}
        >
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                <div
                    style={{
                        width: 8,
                        height: 8,
                        borderRadius: "50%",
                        background: resultColor,
                        flexShrink: 0,
                    }}
                />
                <div style={{ flex: 1 }}>
                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 500, color: "var(--text-primary)" }}>
                        {item.label}
                    </div>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 1 }}>
                        {item.last_run_at
                            ? `Last: ${new Date(item.last_run_at).toLocaleTimeString()}`
                            : "Never run"}
                        {item.interval_minutes > 0 && ` (every ${item.interval_minutes}m)`}
                    </div>
                </div>
            </div>
            {item.last_message && item.last_result !== "ok" && (
                <div style={{ fontSize: "var(--text-xs)", color: resultColor, marginTop: "var(--space-2)", paddingLeft: 16 }}>
                    {item.last_message.slice(0, 200)}
                </div>
            )}
        </div>
    );
}

const sectionLabelStyle: CSSProperties = {
    fontSize: "var(--text-xs)",
    color: "var(--text-muted)",
    marginBottom: "var(--space-2)",
    fontWeight: 600,
};

const detailLabelStyle: CSSProperties = {
    fontSize: "var(--text-xs)",
    color: "var(--text-muted)",
};

const detailBodyStyle: CSSProperties = {
    padding: "var(--space-2)",
    borderRadius: "var(--radius-sm)",
    background: "var(--bg-tertiary)",
    fontSize: "var(--text-sm)",
    color: "var(--text-primary)",
    marginTop: 4,
    whiteSpace: "pre-wrap",
};

const inputRowStyle: CSSProperties = {
    padding: "var(--space-2) var(--space-3)",
    borderRadius: "var(--radius-md)",
    border: "1px solid var(--border)",
    background: "var(--bg-tertiary)",
    color: "var(--text-primary)",
    fontSize: "var(--text-sm)",
    outline: "none",
};

const inputBlockStyle: CSSProperties = {
    ...inputRowStyle,
    resize: "vertical",
    fontFamily: "inherit",
};
