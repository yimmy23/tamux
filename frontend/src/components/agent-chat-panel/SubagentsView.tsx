import { useCallback, useEffect, useMemo, useState } from "react";
import { allLeafIds, findLeaf } from "../../lib/bspTree";
import { fetchAgentRuns, formatRunStatus, formatRunTimestamp, isRunActive, isSubagentRun, runStatusColor, type AgentRun } from "../../lib/agentRuns";
import { fetchThreadTodos } from "../../lib/agentTodos";
import { useAgentStore } from "../../lib/agentStore";
import type { Workspace } from "../../lib/types";
import { shortenHomePath, useWorkspaceStore } from "../../lib/workspaceStore";
import { ActionButton, EmptyPanel, MetricRibbon, SectionTitle } from "./shared";

type SubagentsViewProps = {
    onOpenThreadView?: () => void;
    onOpenTasksView?: () => void;
};

type TaskWorkspaceLocation = {
    workspaceId: string;
    workspaceName: string;
    surfaceId: string;
    surfaceName: string;
    paneId: string;
    cwd: string | null;
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
};

type RemoteAgentThread = {
    id: string;
    title: string;
    messages: RemoteAgentMessage[];
};

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

export function SubagentsView({ onOpenThreadView, onOpenTasksView }: SubagentsViewProps) {
    const [runs, setRuns] = useState<AgentRun[]>([]);
    const amux = (window as any).tamux ?? (window as any).amux;
    const workspaces = useWorkspaceStore((state) => state.workspaces);
    const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
    const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
    const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
    const focusCanvasPanel = useWorkspaceStore((state) => state.focusCanvasPanel);
    const createThread = useAgentStore((state) => state.createThread);
    const addMessage = useAgentStore((state) => state.addMessage);
    const setActiveThread = useAgentStore((state) => state.setActiveThread);
    const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
    const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
    const threads = useAgentStore((state) => state.threads);

    const refreshRuns = useCallback(async () => {
        const result = await fetchAgentRuns();
        setRuns(result);
    }, []);

    useEffect(() => {
        void refreshRuns();
        const interval = window.setInterval(() => {
            void refreshRuns();
        }, 3000);
        return () => window.clearInterval(interval);
    }, [refreshRuns]);

    const subagents = useMemo(
        () => runs.filter(isSubagentRun),
        [runs],
    );
    const activeCount = subagents.filter(isRunActive).length;
    const withChatCount = subagents.filter((run) => Boolean(run.thread_id)).length;
    const runtimeCount = new Set(subagents.map((run) => run.runtime ?? "daemon")).size;
    const groupedSubagents = useMemo(() => {
        const groups = new Map<string, AgentRun[]>();
        for (const run of subagents) {
            const key = run.parent_run_id
                ? `task:${run.parent_run_id}`
                : run.parent_thread_id
                    ? `thread:${run.parent_thread_id}`
                    : "unscoped";
            const bucket = groups.get(key) ?? [];
            bucket.push(run);
            groups.set(key, bucket);
        }
        return Array.from(groups.entries())
            .map(([key, items]) => ({
                key,
                items: items.slice().sort((a, b) => b.created_at - a.created_at),
                parentTitle: items[0]?.parent_title ?? null,
            }))
            .sort((a, b) => b.items[0]!.created_at - a.items[0]!.created_at);
    }, [subagents]);

    const openRunSession = useCallback((run: AgentRun) => {
        const location = findTaskWorkspaceLocation(workspaces, run.session_id);
        if (!location) {
            return;
        }
        setActiveWorkspace(location.workspaceId);
        setActiveSurface(location.surfaceId);
        focusCanvasPanel(location.paneId, { storePreviousView: true });
        setActivePaneId(location.paneId);
    }, [focusCanvasPanel, setActivePaneId, setActiveSurface, setActiveWorkspace, workspaces]);

    const openRunThread = useCallback(async (run: AgentRun) => {
        if (!run.thread_id || !amux?.agentGetThread) {
            return;
        }

        const existingThread = threads.find((entry) => entry.daemonThreadId === run.thread_id);
        if (existingThread) {
            setActiveThread(existingThread.id);
            onOpenThreadView?.();
            return;
        }

        const remoteThread = await amux.agentGetThread(run.thread_id) as RemoteAgentThread | null;
        if (!remoteThread) {
            return;
        }

        const location = findTaskWorkspaceLocation(workspaces, run.session_id);
        const localThreadId = createThread({
            workspaceId: location?.workspaceId ?? null,
            surfaceId: location?.surfaceId ?? null,
            paneId: location?.paneId ?? null,
            title: remoteThread.title || run.title,
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
    }, [addMessage, amux, createThread, onOpenThreadView, setActiveThread, setThreadDaemonId, setThreadTodos, threads, workspaces]);

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <MetricRibbon
                items={[
                    { label: "Subagents", value: String(subagents.length) },
                    { label: "Active", value: String(activeCount), accent: activeCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "With Chat", value: String(withChatCount), accent: withChatCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Runtimes", value: String(runtimeCount) },
                ]}
            />

            <SectionTitle title="Subagents" subtitle="Dedicated child-agent view grouped by parent task or thread" />

            {subagents.length === 0 ? (
                <EmptyPanel message="No subagents have been spawned yet. Parent tasks can create them with spawn_subagent." />
            ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
                    {groupedSubagents.map((group) => (
                        <div
                            key={group.key}
                            style={{
                                border: "1px solid var(--border)",
                                borderRadius: "var(--radius-lg)",
                                background: "var(--bg-secondary)",
                                padding: "var(--space-3)",
                            }}
                        >
                            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
                                <div>
                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>
                                        {group.key.startsWith("task:")
                                            ? group.parentTitle || `Parent Task ${group.key.slice(5)}`
                                            : `Parent Thread ${group.key.slice(7)}`}
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
                                        {group.items.filter(isRunActive).length} active · {group.items.length} total
                                    </div>
                                </div>
                                {group.key.startsWith("task:") && onOpenTasksView && (
                                    <ActionButton onClick={onOpenTasksView}>Open Tasks</ActionButton>
                                )}
                            </div>

                            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                                {group.items.map((run) => {
                                    const location = findTaskWorkspaceLocation(workspaces, run.session_id);
                                    return (
                                        <div
                                            key={run.id}
                                            style={{
                                                borderRadius: "var(--radius-md)",
                                                border: `1px solid ${runStatusColor(run.status)}`,
                                                background: "var(--bg-tertiary)",
                                                padding: "var(--space-3)",
                                            }}
                                        >
                                            <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)", flexWrap: "wrap" }}>
                                                <div style={{ minWidth: 0, flex: 1 }}>
                                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>{run.title}</div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                                                        {formatRunStatus(run)} · runtime {run.runtime ?? "daemon"} · {formatRunTimestamp(run.created_at)}
                                                        {run.classification ? ` · ${run.classification}` : ""}
                                                    </div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-2)" }}>
                                                        {run.description}
                                                    </div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
                                                        {run.thread_id ? `thread ${run.thread_id}` : "no chat thread yet"}
                                                        {run.session_id ? ` · session ${run.session_id}` : ""}
                                                        {location?.cwd ? ` · ${shortenHomePath(location.cwd)}` : ""}
                                                    </div>
                                                </div>
                                                <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                                    {run.thread_id && (
                                                        <ActionButton onClick={() => void openRunThread(run)}>Open Chat</ActionButton>
                                                    )}
                                                    {location && (
                                                        <ActionButton onClick={() => openRunSession(run)}>Open Session</ActionButton>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
