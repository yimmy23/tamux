import { TraceView } from "@/components/agent-chat-panel/TraceView";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAuditStore } from "@/lib/auditStore";
import { useNotificationStore } from "@/lib/notificationStore";

export function ActivityRail() {
  const notifications = useNotificationStore((state) => state.notifications);
  const auditEntries = useAuditStore((state) => state.entries);
  const { pendingApprovals, scopedOperationalEvents } = useAgentChatPanelRuntime();

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-metric-card"><strong>{pendingApprovals.length}</strong><span>Approvals</span></div>
      <div className="zorai-metric-card"><strong>{scopedOperationalEvents.length}</strong><span>Ops events</span></div>
      <div className="zorai-metric-card"><strong>{notifications.length}</strong><span>Notifications</span></div>
      <div className="zorai-metric-card"><strong>{auditEntries.length}</strong><span>Audit entries</span></div>
    </div>
  );
}

export function ActivityView() {
  const runtime = useAgentChatPanelRuntime();

  return (
    <section className="zorai-feature-surface">
      <TraceView
        operationalEvents={runtime.scopedOperationalEvents}
        cognitiveEvents={runtime.scopedCognitiveEvents}
        pendingApprovals={runtime.pendingApprovals}
        todosByThread={runtime.daemonTodosByThread}
        goalRuns={runtime.goalRunsForTrace}
      />
    </section>
  );
}
