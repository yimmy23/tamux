import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useAgentStore } from "../lib/agentStore";
import { useStatusStore, type AgentActivityState } from "../lib/statusStore";
import { useTierStore } from "../lib/tierStore";
import { InlineSystemMonitor } from "./status-bar/InlineSystemMonitor";
import { StatusIndicator } from "./status-bar/StatusPrimitives";
import { TaskTrayButton } from "./TaskTray";
import { dividerStyle, statusBarRootStyle } from "./status-bar/shared";

const ACTIVITY_DISPLAY: Record<AgentActivityState, { label: string; status: "success" | "warning" | "neutral" | "info" }> = {
  idle: { label: "idle", status: "neutral" },
  thinking: { label: "thinking...", status: "info" },
  executing_tool: { label: "running tool", status: "info" },
  waiting_for_approval: { label: "awaiting approval", status: "warning" },
  running_goal: { label: "running goal", status: "success" },
  goal_running: { label: "running goal", status: "success" },
};

export function StatusBar() {
  const ws = useWorkspaceStore((s) => s.activeWorkspace());
  const zoomedPaneId = useWorkspaceStore((s) => s.zoomedPaneId);
  const toggleNotificationPanel = useWorkspaceStore((s) => s.toggleNotificationPanel);
  const unreadCount = useNotificationStore((s) => s.unreadCount);
  const themeName = useSettingsStore((s) => s.settings.themeName);
  const sandboxEnabled = useSettingsStore((s) => s.settings.sandboxEnabled);
  const snapshotBackend = useSettingsStore((s) => s.settings.snapshotBackend);
  const gatewayEnabled = useAgentStore((s) => s.agentSettings.gateway_enabled);
  const activity = useStatusStore((s) => s.activity);
  const activeGoalRunTitle = useStatusStore((s) => s.activeGoalRunTitle);
  const providerHealth = useStatusStore((s) => s.providerHealth);
  const gatewayStatuses = useStatusStore((s) => s.gatewayStatuses);
  const diagnostics = useStatusStore((s) => s.diagnostics);
  const currentTier = useTierStore((s) => s.currentTier);
  const [daemonConnected, setDaemonConnected] = useState(false);
  const [userHoveringStatus, setUserHoveringStatus] = useState(false);
  const activityInfo = ACTIVITY_DISPLAY[activity] ?? ACTIVITY_DISPLAY.idle;
  const activityLabel = (activity === "running_goal" || activity === "goal_running") && activeGoalRunTitle
    ? `goal: ${activeGoalRunTitle}`
    : activityInfo.label;
  const unhealthyProviderCount = useMemo(
    () => providerHealth.filter((p) => !p.canExecute).length,
    [providerHealth],
  );
  const gatewayFailureCount = useMemo(
    () =>
      gatewayStatuses.filter(
        (gateway) =>
          gateway.status.toLowerCase() === "error" ||
          (gateway.consecutiveFailures ?? 0) > 0,
      ).length,
    [gatewayStatuses],
  );

  useEffect(() => {
    async function check() {
      try {
        if (typeof window !== "undefined" && "zorai" in window) {
          const ok = await getBridge()?.checkDaemon?.();
          setDaemonConnected(ok ?? false);
        }
      } catch {
        setDaemonConnected(false);
      }
    }
    check();
    const interval = setInterval(check, 10_000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div style={statusBarRootStyle}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", minWidth: 0 }}>

        {currentTier && currentTier !== "newcomer" && (
          <span
            style={{
              color: userHoveringStatus ? "var(--white)" : "var(--text-muted)",
              fontSize: "var(--text-xs)",
              textTransform: "capitalize",
              border: "1px solid var(--glass-border)",
              padding: "2px 6px",
              borderRadius: "var(--radius-full)",
              opacity: userHoveringStatus ? 1 : 0.7,
              transition: "opacity var(--transition-fast)",
            }}
            onMouseEnter={() => setUserHoveringStatus(true)}
            onMouseLeave={() => setUserHoveringStatus(false)}
          >
            {currentTier}
          </span>
        )}
        <StatusIndicator
          label="daemon"
          status={daemonConnected ? "success" : "neutral"}
        />

        {gatewayEnabled && (
          <StatusIndicator label="gateway" status="success" />
        )}

        {daemonConnected && (
          <StatusIndicator
            label={activityLabel}
            status={activityInfo.status}
          />
        )}
        {unhealthyProviderCount > 0 && (
          <StatusIndicator
            label={`providers degraded ${unhealthyProviderCount}`}
            status="warning"
          />
        )}
        {gatewayFailureCount > 0 && (
          <StatusIndicator
            label={`gateway degraded ${gatewayFailureCount}`}
            status="warning"
          />
        )}
        {diagnostics.operatorProfileSchedulerFallback && (
          <StatusIndicator
            label="profile scheduler fallback"
            status="warning"
          />
        )}
        {diagnostics.operatorProfileSyncDirty && (
          <StatusIndicator
            label={`profile sync ${diagnostics.operatorProfileSyncState}`}
            status="warning"
          />
        )}
        {diagnostics.skillMeshBackend === "mesh" && (
          <StatusIndicator
            label={`mesh ${diagnostics.skillMeshState}`}
            status={diagnostics.skillMeshState === "fresh" ? "success" : "warning"}
          />
        )}

        {sandboxEnabled && (
          <StatusIndicator label="sandbox" status="success" />
        )}

        {zoomedPaneId && (
          <StatusIndicator label="zoomed" status="warning" />
        )}
        {snapshotBackend !== "tar" && (
          <StatusIndicator label={snapshotBackend} status="success" />
        )}


        {ws?.gitBranch && (
          <span style={{ opacity: 0.8 }}>
            ⎇ {ws.gitBranch}
            {ws.gitDirty && (
              <span style={{ color: "var(--warning)", marginLeft: 2 }}>●</span>
            )}
          </span>
        )}

        {ws && ws.listeningPorts.length > 0 && (
          <span style={{ opacity: 0.7 }}>
            :{ws.listeningPorts.join(",")}
          </span>
        )}
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>


        <div style={dividerStyle} />

        <TaskTrayButton />

        <div style={dividerStyle} />

        <button
          type="button"
          onClick={toggleNotificationPanel}
          title="Open notifications"
          style={{
            border: "1px solid var(--glass-border)",
            background: unreadCount > 0 ? "var(--approval-soft)" : "transparent",
            color: unreadCount > 0 ? "var(--warning)" : "var(--text-secondary)",
            fontSize: "var(--text-xs)",
            fontWeight: 700,
            padding: "3px 8px",
            cursor: "pointer",
          }}
        >
          Alerts {unreadCount > 0 ? `(${unreadCount})` : ""}
        </button>

        {unreadCount > 0 && (
          <span
            style={{
              color: "var(--accent)",
              marginLeft: "var(--space-2)",
              fontSize: "var(--text-sm)",
            }}
          >
            {unreadCount}
          </span>
        )}

        {currentTier !== "newcomer" && (
          <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", opacity: 0.7 }}>
            {currentTier.replace("_", " ")}
          </span>
        )}

        <span style={{ marginLeft: "var(--space-3)", fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.1em" }}>
          {themeName}
        </span>

        <InlineSystemMonitor />
      </div>
    </div>
  );
}
