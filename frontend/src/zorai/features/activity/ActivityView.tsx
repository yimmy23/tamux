import { useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAuditStore } from "@/lib/auditStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { UsagePanel } from "./ActivityUsagePanel";
import { buildUsageStats, formatCount } from "./ActivityUsageStats";

type ActivityTab = "timeline" | "reasoning" | "planner" | "usage";

export function ActivityRail() {
  const notifications = useNotificationStore((state) => state.notifications);
  const auditEntries = useAuditStore((state) => state.entries);
  const { pendingApprovals, scopedOperationalEvents } = useAgentChatPanelRuntime();

  return (
    <div className="zorai-rail-stack">
      <Metric label="Approvals" value={pendingApprovals.length} />
      <Metric label="Ops events" value={scopedOperationalEvents.length} />
      <Metric label="Notifications" value={notifications.length} />
      <Metric label="Audit entries" value={auditEntries.length} />
    </div>
  );
}

export function ActivityView() {
  const runtime = useAgentChatPanelRuntime();
  const [tab, setTab] = useState<ActivityTab>("timeline");
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();

  const operationalEvents = useMemo(() => {
    return runtime.scopedOperationalEvents.filter((event) => {
      if (!normalizedQuery) return true;
      return [event.kind, event.command ?? "", event.message ?? "", event.blastRadius ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
  }, [normalizedQuery, runtime.scopedOperationalEvents]);

  const cognitiveEvents = useMemo(() => {
    return runtime.scopedCognitiveEvents.filter((event) => {
      if (!normalizedQuery) return true;
      return [event.source, event.content].join(" ").toLowerCase().includes(normalizedQuery);
    });
  }, [normalizedQuery, runtime.scopedCognitiveEvents]);

  const todoThreads = useMemo(() => {
    return Object.entries(runtime.daemonTodosByThread).filter(([, todos]) => todos.length > 0);
  }, [runtime.daemonTodosByThread]);

  const usageStats = useMemo(() => {
    return buildUsageStats(runtime.threads, runtime.allMessagesByThread, runtime.goalRunsForTrace);
  }, [runtime.allMessagesByThread, runtime.goalRunsForTrace, runtime.threads]);

  return (
    <section className="zorai-feature-surface zorai-activity-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Activity</div>
          <h1>Follow approvals, events, and planner state.</h1>
          <p>Activity is the operational timeline for Zorai runs: what happened, what is pending, and what agents are planning next.</p>
        </div>
      </div>

      <div className="zorai-metric-grid">
        <Metric label="Pending approvals" value={runtime.pendingApprovals.length} />
        <Metric label="Operational events" value={runtime.scopedOperationalEvents.length} />
        <Metric label="Reasoning events" value={runtime.scopedCognitiveEvents.length} />
        <Metric label="Usage tokens" value={formatCount(usageStats.totals.totalTokens)} />
      </div>

      <div className="zorai-toolbar">
        <input
          className="zorai-input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search activity..."
        />
        {(["timeline", "reasoning", "planner", "usage"] as const).map((nextTab) => (
          <button
            type="button"
            key={nextTab}
            className={["zorai-ghost-button", tab === nextTab ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
            onClick={() => setTab(nextTab)}
          >
            {nextTab}
          </button>
        ))}
      </div>

      {tab === "timeline" ? (
        <div className="zorai-activity-grid">
          <ActivityColumn title="Pending Approvals">
            {runtime.pendingApprovals.length === 0 ? <EmptyActivity text="No approvals are waiting." /> : (
              runtime.pendingApprovals.slice(0, 8).map((approval) => (
                <ActivityItem key={approval.id} title={approval.command || approval.id} meta={approval.status} body={approval.reasons.join("\n") || approval.blastRadius || "Approval request"} />
              ))
            )}
          </ActivityColumn>
          <ActivityColumn title="Operational Timeline">
            {operationalEvents.length === 0 ? <EmptyActivity text="No operational events match." /> : (
              operationalEvents.slice(0, 16).map((event) => (
                <ActivityItem
                  key={event.id}
                  title={event.kind}
                  meta={formatTime(event.timestamp)}
                  body={event.command || event.message || event.blastRadius || "Runtime event"}
                />
              ))
            )}
          </ActivityColumn>
        </div>
      ) : null}

      {tab === "reasoning" ? (
        <div className="zorai-panel">
          <div className="zorai-section-label">Reasoning Trace</div>
          {cognitiveEvents.length === 0 ? <EmptyActivity text="No reasoning events match." /> : (
            cognitiveEvents.slice(0, 20).map((event) => (
              <ActivityItem key={event.id} title={event.source} meta={formatTime(event.timestamp)} body={event.content} />
            ))
          )}
        </div>
      ) : null}

      {tab === "planner" ? (
        <div className="zorai-activity-grid">
          <ActivityColumn title="Planner Todos">
            {todoThreads.length === 0 ? <EmptyActivity text="No active planner todos." /> : (
              todoThreads.map(([threadId, todos]) => (
                <ActivityItem
                  key={threadId}
                  title={`Thread ${threadId}`}
                  meta={`${todos.length} items`}
                  body={todos.map((todo) => `${todo.status}: ${todo.content}`).join("\n")}
                />
              ))
            )}
          </ActivityColumn>
          <ActivityColumn title="Goal Events">
            {runtime.goalRunsForTrace.length === 0 ? <EmptyActivity text="No goal events are loaded." /> : (
              runtime.goalRunsForTrace.slice(0, 10).map((goal) => (
                <ActivityItem
                  key={goal.id}
                  title={goal.title || goal.goal}
                  meta={goal.status}
                  body={(goal.events ?? []).slice(-3).map((event) => event.message).join("\n") || goal.goal}
                />
              ))
            )}
          </ActivityColumn>
        </div>
      ) : null}

      {tab === "usage" ? <UsagePanel stats={usageStats} /> : null}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="zorai-metric-card">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function ActivityColumn({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="zorai-panel">
      <div className="zorai-section-label">{title}</div>
      <div className="zorai-activity-list">{children}</div>
    </div>
  );
}

function ActivityItem({ title, meta, body }: { title: string; meta: string; body: string }) {
  return (
    <article className="zorai-activity-item">
      <div>
        <strong>{title}</strong>
        <span>{meta}</span>
      </div>
      <p>{body}</p>
    </article>
  );
}

function EmptyActivity({ text }: { text: string }) {
  return <div className="zorai-empty-state">{text}</div>;
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp) ? new Date(timestamp).toLocaleTimeString() : "pending";
}
