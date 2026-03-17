import { useCallback, useEffect, useState } from "react";
import { SectionTitle, ActionButton, iconButtonStyle } from "./shared";
import {
    fetchAgentTasks,
    formatTaskStatus,
    formatTaskTimestamp,
    isTaskActive,
    taskStatusColor,
    type AgentQueueTask,
} from "../../lib/agentTaskQueue";

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

export function TasksView() {
    const [tasks, setTasks] = useState<AgentQueueTask[]>([]);
    const [heartbeatItems, setHeartbeatItems] = useState<HeartbeatItem[]>([]);
    const [newTaskTitle, setNewTaskTitle] = useState("");
    const [newTaskDescription, setNewTaskDescription] = useState("");
    const [newTaskCommand, setNewTaskCommand] = useState("");
    const [newTaskSessionId, setNewTaskSessionId] = useState("");
    const [newTaskDependencies, setNewTaskDependencies] = useState("");
    const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

    const amux = (window as any).tamux ?? (window as any).amux;

    const refreshTasks = useCallback(async () => {
        const result = await fetchAgentTasks();
        setTasks(result);
        setSelectedTaskId((current) => current ?? result[0]?.id ?? null);
    }, []);

    const refreshHeartbeat = useCallback(async () => {
        if (!amux?.agentHeartbeatGetItems) return;
        try {
            const result = await amux.agentHeartbeatGetItems();
            setHeartbeatItems(Array.isArray(result) ? result : []);
        } catch { /* silent */ }
    }, [amux]);

    useEffect(() => {
        refreshTasks();
        refreshHeartbeat();
        const interval = setInterval(() => {
            refreshTasks();
            refreshHeartbeat();
        }, 5000);
        return () => clearInterval(interval);
    }, [refreshTasks, refreshHeartbeat]);

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
        refreshTasks();
    };

    const cancelTask = async (taskId: string) => {
        if (!amux?.agentCancelTask) return;
        await amux.agentCancelTask(taskId);
        refreshTasks();
    };

    const activeTasks = tasks.filter(isTaskActive);
    const completedTasks = tasks.filter((task) => !isTaskActive(task));
    const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? tasks[0] ?? null;

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <SectionTitle title="Task Queue" subtitle="Autonomous task execution by daemon agent" />

            {/* Add task form */}
            <div style={{ marginBottom: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                <input
                    type="text"
                    placeholder="Task title..."
                    value={newTaskTitle}
                    onChange={(e) => setNewTaskTitle(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && addTask()}
                    style={{
                        padding: "var(--space-2) var(--space-3)",
                        borderRadius: "var(--radius-md)",
                        border: "1px solid var(--border)",
                        background: "var(--bg-tertiary)",
                        color: "var(--text-primary)",
                        fontSize: "var(--text-sm)",
                        outline: "none",
                    }}
                />
                {newTaskTitle && (
                    <textarea
                        placeholder="Description (optional)..."
                        value={newTaskDescription}
                        onChange={(e) => setNewTaskDescription(e.target.value)}
                        rows={2}
                        style={{
                            padding: "var(--space-2) var(--space-3)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--border)",
                            background: "var(--bg-tertiary)",
                            color: "var(--text-primary)",
                            fontSize: "var(--text-xs)",
                            outline: "none",
                            resize: "vertical",
                            fontFamily: "inherit",
                        }}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Preferred command or entrypoint (optional)..."
                        value={newTaskCommand}
                        onChange={(e) => setNewTaskCommand(e.target.value)}
                        style={{
                            padding: "var(--space-2) var(--space-3)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--border)",
                            background: "var(--bg-tertiary)",
                            color: "var(--text-primary)",
                            fontSize: "var(--text-xs)",
                            outline: "none",
                        }}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Target session ID (optional)..."
                        value={newTaskSessionId}
                        onChange={(e) => setNewTaskSessionId(e.target.value)}
                        style={{
                            padding: "var(--space-2) var(--space-3)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--border)",
                            background: "var(--bg-tertiary)",
                            color: "var(--text-primary)",
                            fontSize: "var(--text-xs)",
                            outline: "none",
                        }}
                    />
                )}
                {newTaskTitle && (
                    <input
                        type="text"
                        placeholder="Dependencies: task IDs, comma-separated (optional)..."
                        value={newTaskDependencies}
                        onChange={(e) => setNewTaskDependencies(e.target.value)}
                        style={{
                            padding: "var(--space-2) var(--space-3)",
                            borderRadius: "var(--radius-md)",
                            border: "1px solid var(--border)",
                            background: "var(--bg-tertiary)",
                            color: "var(--text-primary)",
                            fontSize: "var(--text-xs)",
                            outline: "none",
                        }}
                    />
                )}
                {newTaskTitle && (
                    <ActionButton onClick={addTask}>Add Task</ActionButton>
                )}
            </div>

            {/* Active tasks */}
            {activeTasks.length > 0 && (
                <div style={{ marginBottom: "var(--space-4)" }}>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginBottom: "var(--space-2)", fontWeight: 600 }}>
                        Active ({activeTasks.length})
                    </div>
                    {activeTasks.map((task) => (
                        <TaskCard key={task.id} task={task} selected={task.id === selectedTask?.id} onSelect={() => setSelectedTaskId(task.id)} onCancel={() => cancelTask(task.id)} />
                    ))}
                </div>
            )}

            {/* Completed tasks */}
            {completedTasks.length > 0 && (
                <div style={{ marginBottom: "var(--space-4)" }}>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginBottom: "var(--space-2)", fontWeight: 600 }}>
                        History ({completedTasks.length})
                    </div>
                    {completedTasks.slice(0, 20).map((task) => (
                        <TaskCard key={task.id} task={task} selected={task.id === selectedTask?.id} onSelect={() => setSelectedTaskId(task.id)} />
                    ))}
                </div>
            )}

            {tasks.length === 0 && (
                <div style={{ textAlign: "center", padding: "var(--space-6)", color: "var(--text-muted)", fontSize: "var(--text-sm)" }}>
                    No tasks yet. Add a task above or tell the agent what to do.
                </div>
            )}

            {selectedTask && (
                <div style={{ marginBottom: "var(--space-5)" }}>
                    <SectionTitle title="Post-Mortem" subtitle="Latest trajectory for the selected task" />
                    <TaskPostMortem task={selectedTask} />
                </div>
            )}

            {/* Heartbeat section */}
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
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
                        <span style={{ color: statusColor, fontWeight: 600 }}>{formatTaskStatus(task)}</span>
                        {task.status === "in_progress" && task.progress > 0 && (
                            <span> {task.progress}%</span>
                        )}
                        <span style={{ marginLeft: "var(--space-2)" }}>
                            {formatTaskTimestamp(task.created_at)}
                        </span>
                        {typeof task.retry_count === "number" && typeof task.max_retries === "number" && (
                            <span style={{ marginLeft: "var(--space-2)" }}>retry {task.retry_count}/{task.max_retries}</span>
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

function TaskPostMortem({ task }: { task: AgentQueueTask }) {
    const logs = [...(task.logs ?? [])].slice(-8).reverse();

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
            {task.dependencies && task.dependencies.length > 0 && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                    Depends on: {task.dependencies.join(", ")}
                </div>
            )}
            {task.last_error && (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
                    {task.last_error}
                </div>
            )}
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
