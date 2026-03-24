import { useEffect, useMemo, useState } from "react";
import { ReasoningStream } from "../ReasoningStream";
import { useAgentMissionStore } from "../../lib/agentMissionStore";
import type { AgentTodoItem } from "../../lib/agentStore";
import type { GoalRun, GoalRunEvent } from "../../lib/goalRuns";
import { DEFAULT_PAGE_SIZE, EmptyPanel, MetricRibbon, PageSizeSelect, PaginationControls, SectionTitle, inputStyle } from "./shared";

export function TraceView({
    operationalEvents,
    cognitiveEvents,
    pendingApprovals,
    todosByThread,
    goalRuns,
}: {
    operationalEvents: ReturnType<typeof useAgentMissionStore.getState>["operationalEvents"];
    cognitiveEvents: ReturnType<typeof useAgentMissionStore.getState>["cognitiveEvents"];
    pendingApprovals: ReturnType<typeof useAgentMissionStore.getState>["approvals"];
    todosByThread: Record<string, AgentTodoItem[]>;
    goalRuns: GoalRun[];
}) {
    const [searchQuery, setSearchQuery] = useState("");
    const [dateFilter, setDateFilter] = useState("");
    const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
    const [reasoningPage, setReasoningPage] = useState(1);
    const [plannerPage, setPlannerPage] = useState(1);
    const [timelinePage, setTimelinePage] = useState(1);

    const matchesDate = (timestamp: number) => {
        if (!dateFilter) return true;
        const date = new Date(timestamp);
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, "0");
        const day = String(date.getDate()).padStart(2, "0");
        return `${year}-${month}-${day}` === dateFilter;
    };

    const normalizedQuery = searchQuery.trim().toLowerCase();

    const filteredCognitiveEvents = useMemo(() => {
        return cognitiveEvents.filter((event) => {
            if (!matchesDate(event.timestamp)) return false;
            if (!normalizedQuery) return true;

            return [event.source, event.content]
                .join(" ")
                .toLowerCase()
                .includes(normalizedQuery);
        });
    }, [cognitiveEvents, dateFilter, normalizedQuery]);

    const filteredOperationalEvents = useMemo(() => {
        return operationalEvents.filter((event) => {
            if (!matchesDate(event.timestamp)) return false;
            if (!normalizedQuery) return true;

            return [event.kind, event.command ?? "", event.message ?? "", event.blastRadius ?? ""]
                .join(" ")
                .toLowerCase()
                .includes(normalizedQuery);
        });
    }, [operationalEvents, dateFilter, normalizedQuery]);

    useEffect(() => {
        setReasoningPage(1);
        setPlannerPage(1);
        setTimelinePage(1);
    }, [searchQuery, dateFilter, pageSize]);

    const visibleCognitiveEvents = useMemo(() => {
        const start = (reasoningPage - 1) * pageSize;
        return filteredCognitiveEvents.slice(start, start + pageSize);
    }, [filteredCognitiveEvents, pageSize, reasoningPage]);

    const visibleOperationalEvents = useMemo(() => {
        const start = (timelinePage - 1) * pageSize;
        return filteredOperationalEvents.slice(start, start + pageSize);
    }, [filteredOperationalEvents, pageSize, timelinePage]);

    const filteredTodoEntries = useMemo(() => {
        return Object.entries(todosByThread).filter(([, items]) => {
            if (!normalizedQuery) return items.length > 0;
            return items.some((item) => item.content.toLowerCase().includes(normalizedQuery));
        });
    }, [normalizedQuery, todosByThread]);

    const filteredPlannerEvents = useMemo(() => {
        return goalRuns
            .flatMap((goalRun) => (goalRun.events ?? []).map((event) => ({ event, goalRun })))
            .filter(({ event, goalRun }) => {
                if (!matchesDate(event.timestamp)) return false;
                if (!normalizedQuery) return true;
                return [
                    goalRun.title,
                    goalRun.goal,
                    event.phase,
                    event.message,
                    event.details ?? "",
                    ...event.todo_snapshot.map((item) => item.content),
                ]
                    .join(" ")
                    .toLowerCase()
                    .includes(normalizedQuery);
            })
            .sort((a, b) => b.event.timestamp - a.event.timestamp);
    }, [dateFilter, goalRuns, normalizedQuery]);

    const visiblePlannerEvents = useMemo(() => {
        const start = (plannerPage - 1) * pageSize;
        return filteredPlannerEvents.slice(start, start + pageSize);
    }, [filteredPlannerEvents, pageSize, plannerPage]);

    return (
        <div style={{ padding: "var(--space-4)", height: "100%", overflow: "auto" }}>
            <MetricRibbon
                items={[
                    { label: "Ops", value: String(operationalEvents.length) },
                    { label: "Reasoning", value: String(cognitiveEvents.length) },
                    { label: "Pending", value: String(pendingApprovals.length) },
                    { label: "Plans", value: String(filteredTodoEntries.length) },
                    { label: "Goals", value: String(goalRuns.length) },
                ]}
            />

            <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap", alignItems: "center", marginBottom: "var(--space-4)" }}>
                <input
                    type="text"
                    value={searchQuery}
                    onChange={(event) => setSearchQuery(event.target.value)}
                    placeholder="Search trace and timeline..."
                    style={{ ...inputStyle, minWidth: 220 }}
                />
                <input
                    type="date"
                    value={dateFilter}
                    onChange={(event) => setDateFilter(event.target.value)}
                    style={{ ...inputStyle, flex: "0 0 auto", minWidth: 170 }}
                />
                <PageSizeSelect value={pageSize} onChange={setPageSize} />
            </div>

            <SectionTitle title="Reasoning Trace" subtitle="Parsed cognitive events" />
            <ReasoningStream events={visibleCognitiveEvents} />
            <div style={{ marginTop: "var(--space-3)" }}>
                <PaginationControls
                    page={reasoningPage}
                    pageSize={pageSize}
                    totalItems={filteredCognitiveEvents.length}
                    onPageChange={setReasoningPage}
                />
            </div>

            <SectionTitle title="Planner State" subtitle="Daemon-managed todo lists across active agent threads" />

            {filteredTodoEntries.length === 0 ? (
                <EmptyPanel message="No planner todos are active yet." />
            ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginBottom: "var(--space-4)" }}>
                    {filteredTodoEntries.map(([threadId, items]) => (
                        <div
                            key={threadId}
                            style={{
                                padding: "var(--space-3)",
                                borderRadius: "var(--radius-lg)",
                                border: "1px solid var(--glass-border)",
                                background: "var(--bg-secondary)",
                            }}
                        >
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginBottom: "var(--space-2)" }}>
                                Thread {threadId}
                            </div>
                            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
                                {items.map((item) => (
                                    <TodoRow key={item.id} item={item} />
                                ))}
                            </div>
                        </div>
                    ))}
                </div>
            )}

            <SectionTitle title="Planner Timeline" subtitle="Goal-run planning, replanning, and todo snapshots over time" />

            {goalRuns.length === 0 ? (
                <EmptyPanel message="No goal-run timeline events captured yet." />
            ) : filteredPlannerEvents.length === 0 ? (
                <EmptyPanel message="No planner events match your current filters." />
            ) : visiblePlannerEvents.length === 0 ? (
                <EmptyPanel message="No planner events on this page. Try a previous page or adjust filters." />
            ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginBottom: "var(--space-4)" }}>
                    {visiblePlannerEvents.map(({ event, goalRun }) => (
                        <PlannerTimelineRow key={`${goalRun.id}:${event.id}`} goalRun={goalRun} event={event} />
                    ))}
                </div>
            )}

            <div style={{ marginTop: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <PaginationControls
                    page={plannerPage}
                    pageSize={pageSize}
                    totalItems={filteredPlannerEvents.length}
                    onPageChange={setPlannerPage}
                />
            </div>

            <SectionTitle title="Operational Timeline" subtitle="Execution events" />

            {operationalEvents.length === 0 ? (
                <EmptyPanel message="No operational events captured yet." />
            ) : filteredOperationalEvents.length === 0 ? (
                <EmptyPanel message="No operational events match your current filters." />
            ) : visibleOperationalEvents.length === 0 ? (
                <EmptyPanel message="No operational events on this page. Try a previous page or adjust filters." />
            ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                    {visibleOperationalEvents.map((event) => (
                        <TimelineRow key={event.id} event={event} />
                    ))}
                </div>
            )}

            <div style={{ marginTop: "var(--space-3)" }}>
                <PaginationControls
                    page={timelinePage}
                    pageSize={pageSize}
                    totalItems={filteredOperationalEvents.length}
                    onPageChange={setTimelinePage}
                />
            </div>
        </div>
    );
}

function TodoRow({ item }: { item: AgentTodoItem }) {
    return (
        <div
            style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
                padding: "6px 8px",
                borderRadius: "var(--radius-sm)",
                background: "var(--bg-tertiary)",
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
    );
}

function todoStatusColor(status: AgentTodoItem["status"]): string {
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

function TimelineRow({ event }: { event: ReturnType<typeof useAgentMissionStore.getState>["operationalEvents"][number] }) {
    return (
        <div
            style={{
                display: "flex",
                gap: "var(--space-3)",
                padding: "var(--space-3)",
                borderRadius: "var(--radius-lg)",
                border: "1px solid var(--glass-border)",
                background: "var(--bg-secondary)",
                alignItems: "flex-start",
            }}
        >
            <div style={{ minWidth: 72, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </div>

            <div style={{ flex: 1 }}>
                <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{event.kind}</div>

                {event.command && (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-1)" }}>
                        {event.command}
                    </div>
                )}
            </div>

            {event.exitCode !== null && (
                <div
                    style={{
                        fontSize: "var(--text-xs)",
                        color: event.exitCode === 0 ? "var(--success)" : "var(--danger)",
                    }}
                >
                    {event.exitCode === 0 ? "✓" : `✗ ${event.exitCode}`}
                </div>
            )}
        </div>
    );
}

function PlannerTimelineRow({ goalRun, event }: { goalRun: GoalRun; event: GoalRunEvent }) {
    const todoSnapshot = event?.todo_snapshot ?? [];

    return (
        <div
            style={{
                display: "flex",
                gap: "var(--space-3)",
                padding: "var(--space-3)",
                borderRadius: "var(--radius-lg)",
                border: "1px solid var(--glass-border)",
                background: "var(--bg-secondary)",
                alignItems: "flex-start",
            }}
        >
            <div style={{ minWidth: 72, fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </div>

            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
                <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{goalRun.title}</div>
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>
                    {event.message}
                </div>
                <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap", fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    <span>{event.phase}</span>
                    {typeof event.step_index === "number" && <span>Step {event.step_index + 1}</span>}
                    <span>{goalRun.status.replace(/_/g, " ")}</span>
                </div>
                {event.details && (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                        {event.details}
                    </div>
                )}
                {todoSnapshot.length > 0 && (
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)", marginTop: "var(--space-1)" }}>
                        {todoSnapshot.map((item) => (
                            <TodoRow key={`${event.id}:${item.id}`} item={{
                                id: item.id,
                                content: item.content,
                                status: item.status,
                                position: item.position,
                                stepIndex: item.step_index ?? null,
                                createdAt: item.created_at ?? null,
                                updatedAt: item.updated_at ?? null,
                            }} />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
