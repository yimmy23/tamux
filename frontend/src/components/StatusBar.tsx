import { useEffect, useMemo, useState } from "react";
import { allLeafIds } from "../lib/bspTree";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useAgentStore } from "../lib/agentStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { useStatusStore } from "../lib/statusStore";
import { useTierStore } from "../lib/tierStore";
import { InlineSystemMonitor } from "./status-bar/InlineSystemMonitor";
import { StatusBarMissionStats } from "./status-bar/StatusBarMissionStats";
import { StatusIndicator } from "./status-bar/StatusPrimitives";
import { TaskTrayButton } from "./TaskTray";
import { Badge, Button, Separator } from "./ui";

export function StatusBar() {
  const ws = useWorkspaceStore((s) => s.activeWorkspace());
  const surface = useWorkspaceStore((s) => s.activeSurface());
  const zoomedPaneId = useWorkspaceStore((s) => s.zoomedPaneId);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const toggleNotificationPanel = useWorkspaceStore((s) => s.toggleNotificationPanel);
  const notifications = useNotificationStore((s) => s.notifications);
  const themeName = useSettingsStore((s) => s.settings.themeName);
  const sandboxEnabled = useSettingsStore((s) => s.settings.sandboxEnabled);
  const snapshotBackend = useSettingsStore((s) => s.settings.snapshotBackend);
  const gatewayEnabled = useAgentStore((s) => s.agentSettings.gateway_enabled);
  const unreadCount = notifications.filter((n) => !n.isRead).length;
  const activity = useStatusStore((s) => s.activity);
  const providerHealth = useStatusStore((s) => s.providerHealth);
  const recentActions = useStatusStore((s) => s.recentActions);
  const currentTier = useTierStore((s) => s.currentTier);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const cognitiveEvents = useAgentMissionStore((s) => s.cognitiveEvents);
  const operationalEvents = useAgentMissionStore((s) => s.operationalEvents);
  const historyHits = useAgentMissionStore((s) => s.historyHits);
  const snapshots = useAgentMissionStore((s) => s.snapshots);
  const [daemonConnected, setDaemonConnected] = useState(false);
  const pendingApprovals = useMemo(() => approvals.filter((entry) => entry.status === "pending").length, [approvals]);
  const traceCount = cognitiveEvents.length;
  const opsCount = operationalEvents.length;
  const toolCallCount = useMemo(() => operationalEvents.filter((e) => e.kind === "tool-call").length, [operationalEvents]);

  useEffect(() => {
    async function check() {
      try {
        if (typeof window !== "undefined" && "amux" in window) {
          const ok = await ((window as any).tamux ?? (window as any).amux).checkDaemon();
          setDaemonConnected(ok);
        }
      } catch {
        setDaemonConnected(false);
      }
    }
    check();
    const interval = setInterval(check, 10_000);
    return () => clearInterval(interval);
  }, []);

  const paneCount = surface ? allLeafIds(surface.layout).length : 0;

  return (
    <div className="flex h-[var(--status-bar-height)] shrink-0 items-center justify-between border-t border-[var(--border)] bg-[var(--bg-secondary)] px-[var(--space-4)] text-[var(--text-xs)] text-[var(--text-secondary)]">
      <div className="flex min-w-0 items-center gap-[var(--space-3)]">
        <StatusIndicator
          label={daemonConnected ? "daemon online" : "daemon offline"}
          status={daemonConnected ? "success" : "neutral"}
        />

        {ws && (
          <span
            className="text-[var(--text-sm)] font-semibold tracking-[0.02em]"
            style={{ color: ws.accentColor }}
          >
            {ws.name}
          </span>
        )}

        {currentTier !== "newcomer" && (
          <Badge variant="default" className="px-[var(--space-2)] py-[2px] text-[var(--text-xs)] capitalize opacity-80">
            {currentTier.replace("_", " ")}
          </Badge>
        )}

        {activity !== "idle" && (
          <StatusIndicator
            label={
              activity === "thinking" ? "thinking" :
              activity === "executing_tool" ? "running tool" :
              activity === "waiting_for_approval" ? "needs approval" :
              activity === "running_goal" || activity === "goal_running" ? "running goal" :
              activity
            }
            status={
              activity === "waiting_for_approval" ? "warning" :
              activity === "thinking" || activity === "executing_tool" ? "success" :
              "neutral"
            }
          />
        )}

        {surface && (
          <span className="text-[var(--text-muted)]">
            {surface.name} · {paneCount} pane{paneCount !== 1 ? "s" : ""}
          </span>
        )}

        {activePaneId && (
          <Badge variant="default" className="amux-code max-w-[12rem] truncate px-[var(--space-2)] py-[2px] opacity-80">
            {activePaneId}
          </Badge>
        )}

        {zoomedPaneId && (
          <StatusIndicator label="zoomed" status="warning" />
        )}

        {sandboxEnabled && (
          <StatusIndicator label="sandbox" status="success" />
        )}

        {snapshotBackend !== "tar" && (
          <StatusIndicator label={snapshotBackend} status="success" />
        )}

        {gatewayEnabled && (
          <StatusIndicator label="gateway" status="success" />
        )}

        {ws?.gitBranch && (
          <Badge variant="default" className="gap-[var(--space-1)] px-[var(--space-2)] py-[2px] opacity-80">
            ⎇ {ws.gitBranch}
            {ws.gitDirty && (
              <span className="ml-[2px] text-[var(--warning)]">●</span>
            )}
          </Badge>
        )}

        {ws && ws.listeningPorts.length > 0 && (
          <Badge variant="default" className="px-[var(--space-2)] py-[2px] opacity-70">
            :{ws.listeningPorts.join(",")}
          </Badge>
        )}
      </div>

      <div className="flex items-center gap-[var(--space-2)]">
        {providerHealth.some((p) => !p.canExecute) && (
          <StatusIndicator
            label={`${providerHealth.filter((p) => !p.canExecute).length} provider(s) tripped`}
            status="warning"
          />
        )}

        <StatusBarMissionStats
          pendingApprovals={pendingApprovals}
          traceCount={traceCount}
          opsCount={opsCount}
          toolCallCount={toolCallCount}
          historyCount={historyHits.length}
          snapshotCount={snapshots.length}
        />

        <Separator orientation="vertical" className="h-4 bg-[var(--border)]" />

        <TaskTrayButton />

        <Separator orientation="vertical" className="h-4 bg-[var(--border)]" />

        <InlineSystemMonitor />

        {recentActions.length > 0 && (
          <span
            className="cursor-default text-[var(--text-xs)] text-[var(--text-secondary)]"
            title={recentActions.slice(0, 3).map((a) => `${a.actionType}: ${a.summary}`).join("\n")}
          >
            {recentActions[0].summary.length > 30
              ? recentActions[0].summary.slice(0, 27) + "..."
              : recentActions[0].summary}
          </span>
        )}

        <Button
          type="button"
          onClick={toggleNotificationPanel}
          title="Open notifications"
          variant={unreadCount > 0 ? "outline" : "ghost"}
          size="sm"
          className={unreadCount > 0 ? "border-[var(--approval-border)] bg-[var(--approval-soft)] text-[var(--warning)]" : ""}
        >
          Alerts {unreadCount > 0 ? `(${unreadCount})` : ""}
        </Button>

        {unreadCount > 0 && (
          <span className="ml-[var(--space-2)] text-[var(--text-sm)] text-[var(--accent)]">
            {unreadCount}
          </span>
        )}

        <span className="ml-[var(--space-3)] text-[var(--text-xs)] uppercase tracking-[0.1em] text-[var(--text-muted)]">
          {themeName}
        </span>
      </div>
    </div>
  );
}
