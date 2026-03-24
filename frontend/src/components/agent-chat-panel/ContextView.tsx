import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { fetchThreadWorkContext, type ThreadWorkContext } from "../../lib/agentWorkContext";
import { shortenHomePath } from "../../lib/workspaceStore";
import { ActionButton, ContextCard, MetricRibbon, SectionTitle, inputStyle, memoryAreaStyle } from "./shared";

type ContextViewProps = {
    agentSettings: {
        active_provider: string;
        context_window_tokens: number;
        context_budget_tokens: number;
    };
    snippets: Array<unknown>;
    transcripts: Array<unknown>;
    scopePaneId: string | null;
    threads: Array<unknown>;
    activeThreadId?: string | null;
    latestContextSnapshot?: { timestamp: number };
    memory: {
        frozenSnapshot: string;
        userProfile: string;
    };
    updateMemory: (field: "frozenSnapshot" | "userProfile", value: string) => void;
    historyQuery: string;
    setHistoryQuery: (value: string) => void;
    historySummary: unknown;
    historyHits: Array<unknown>;
    symbolQuery: string;
    setSymbolQuery: (value: string) => void;
    symbolHits: Array<unknown>;
    scopeController?: {
        searchHistory?: (query: string, limit?: number) => Promise<unknown>;
        generateSkill?: (query?: string, title?: string) => Promise<unknown>;
    } | null;
};

export function ContextView(props: ContextViewProps) {
    const [workContext, setWorkContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });

    useEffect(() => {
        if (!props.activeThreadId) {
            setWorkContext({ threadId: "", entries: [] });
            return;
        }
        let cancelled = false;
        void fetchThreadWorkContext(props.activeThreadId).then((next) => {
            if (!cancelled) {
                setWorkContext(next);
            }
        });
        return () => {
            cancelled = true;
        };
    }, [props.activeThreadId]);

    useEffect(() => {
        const bridge = getBridge();
        const activeThreadId = props.activeThreadId;
        if (!activeThreadId || !bridge?.onAgentEvent) {
            return;
        }
        return bridge.onAgentEvent((event: any) => {
            if (event?.type !== "work_context_update" || event?.thread_id !== activeThreadId) {
                return;
            }
            void fetchThreadWorkContext(activeThreadId).then(setWorkContext);
        });
    }, [props.activeThreadId]);

    const workMetrics = useMemo(() => {
        const changed = workContext.entries.filter((entry) => entry.kind === "repo_change").length;
        const artifacts = workContext.entries.filter((entry) => entry.kind !== "repo_change").length;
        return { changed, artifacts };
    }, [workContext.entries]);

    return (
        <div style={{ padding: "var(--space-4)", height: "100%", overflow: "auto" }}>
            <MetricRibbon
                items={[
                    { label: "Provider", value: props.agentSettings.active_provider },
                    { label: "Skills", value: String(props.snippets.length) },
                    { label: "Transcripts", value: String(props.transcripts.length) },
                    { label: "Changed", value: String(workMetrics.changed) },
                    { label: "Artifacts", value: String(workMetrics.artifacts) },
                ]}
            />

            <SectionTitle title="Live Context" subtitle="Current session envelope" />
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <ContextCard label="Pane" value={props.scopePaneId ?? "none"} />
                <ContextCard label="Threads" value={String(props.threads.length)} />
                <ContextCard label="Context Length" value={`${props.agentSettings.context_window_tokens.toLocaleString()} tok`} />
                <ContextCard label="Token Budget" value={`${props.agentSettings.context_budget_tokens.toLocaleString()} tok`} />
                <ContextCard label="Snapshot Age" value={props.latestContextSnapshot ? new Date(props.latestContextSnapshot.timestamp).toLocaleTimeString() : "n/a"} />
            </div>

            <SectionTitle title="Frozen Snapshot" subtitle={`${props.memory.frozenSnapshot.length}/2200 chars`} />
            <textarea
                value={props.memory.frozenSnapshot}
                onChange={(e) => props.updateMemory("frozenSnapshot", e.target.value)}
                style={memoryAreaStyle}
                maxLength={2200}
            />

            <SectionTitle title="User Profile" subtitle={`${props.memory.userProfile.length}/1375 chars`} />
            <textarea
                value={props.memory.userProfile}
                onChange={(e) => props.updateMemory("userProfile", e.target.value)}
                style={{ ...memoryAreaStyle, minHeight: 120 }}
                maxLength={1375}
            />

            <SectionTitle title="History Recall" subtitle="Search across managed executions" />
            <div style={{ display: "flex", gap: "var(--space-2)", marginBottom: "var(--space-3)" }}>
                <input
                    value={props.historyQuery}
                    onChange={(e) => props.setHistoryQuery(e.target.value)}
                    placeholder="Search history..."
                    style={inputStyle}
                />
                <ActionButton onClick={() => void props.scopeController?.searchHistory?.(props.historyQuery, 8)}>Search</ActionButton>
                <ActionButton onClick={() => void props.scopeController?.generateSkill?.(props.historyQuery || undefined, props.historyQuery ? `${props.historyQuery} workflow` : "Recovered Workflow")}>Extract Skill</ActionButton>
            </div>

            <SectionTitle title="Work Context" subtitle="Recent files and artifacts from the active thread" />
            {workContext.entries.length > 0 ? (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginBottom: "var(--space-3)" }}>
                    {workContext.entries.slice(0, 8).map((entry) => (
                        <div key={`${entry.source}:${entry.path}`} style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-secondary)", border: "1px solid var(--border)" }}>
                            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{entry.changeKind ?? entry.kind ?? "file"}</span>
                                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{entry.source}</span>
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-primary)", fontFamily: "var(--font-mono)", marginTop: 4, wordBreak: "break-word" }}>
                                {entry.repoRoot ? `${shortenHomePath(entry.repoRoot)}/${entry.path}` : shortenHomePath(entry.path)}
                            </div>
                        </div>
                    ))}
                </div>
            ) : (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    No file or artifact activity recorded for the active thread yet.
                </div>
            )}
        </div>
    );
}
