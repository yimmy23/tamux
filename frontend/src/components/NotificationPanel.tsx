import type { CSSProperties } from "react";
import { cn, overlayClassName, panelSurfaceClassName } from "./ui";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { encodeTextToBase64 } from "./terminal-pane/utils";
import { NotificationHeader } from "./notification-panel/NotificationHeader";
import { NotificationList } from "./notification-panel/NotificationList";
import type { TerminalNotification } from "../lib/types";

/**
 * Slide-over notification panel (Ctrl+I).
 * Shows notification history with mark-read and clear actions.
 */
type NotificationPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function NotificationPanel({ style, className }: NotificationPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.notificationPanelOpen);
  const toggle = useWorkspaceStore((s) => s.toggleNotificationPanel);
  const notifications = useNotificationStore((s) => s.notifications);
  const markRead = useNotificationStore((s) => s.markRead);
  const markAllRead = useNotificationStore((s) => s.markAllRead);
  const clearAll = useNotificationStore((s) => s.clearAll);
  const clearPaneNotifications = useNotificationStore((s) => s.clearPaneNotifications);
  const setActiveWorkspace = useWorkspaceStore((s) => s.setActiveWorkspace);
  const setActiveSurface = useWorkspaceStore((s) => s.setActiveSurface);
  const setActivePaneId = useWorkspaceStore((s) => s.setActivePaneId);
  const focusCanvasPanel = useWorkspaceStore((s) => s.focusCanvasPanel);
  const clearCanvasPanelStatus = useWorkspaceStore((s) => s.clearCanvasPanelStatus);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const resolveApproval = useAgentMissionStore((s) => s.resolveApproval);

  const handleSelectNotification = (notification: TerminalNotification) => {
    if (notification.workspaceId) {
      setActiveWorkspace(notification.workspaceId);
    }
    if (notification.surfaceId) {
      setActiveSurface(notification.surfaceId);
    }

    const panelId = notification.panelId ?? notification.paneId;
    if (panelId) {
      focusCanvasPanel(panelId, { storePreviousView: true });
      setActivePaneId(panelId);
    }
  };

  const reactToApprovalNotification = async (
    notification: TerminalNotification,
    decision: "approve" | "deny"
  ) => {
    handleSelectNotification(notification);

    const paneId = notification.panelId ?? notification.paneId;
    if (!paneId) return;

    const amux = (window as any).tamux ?? (window as any).amux;
    const pendingApproval = approvals.find(
      (entry) => entry.paneId === paneId && entry.status === "pending" && entry.handledAt === null
    );

    if (pendingApproval && amux?.resolveManagedApproval) {
      const daemonDecision = decision === "approve" ? "approve-once" : "deny";
      await amux.resolveManagedApproval(paneId, pendingApproval.id, daemonDecision);
      resolveApproval(pendingApproval.id, decision === "approve" ? "approved-once" : "denied");
    } else if (amux?.sendTerminalInput) {
      const response = decision === "approve" ? "y\r" : "n\r";
      await amux.sendTerminalInput(paneId, encodeTextToBase64(response));
    }

    clearPaneNotifications(paneId, "approval");
    clearCanvasPanelStatus(paneId);
  };

  if (!open) return null;

  const unread = notifications.filter((n) => !n.isRead);

  return (
    <div
      onClick={toggle}
      style={style}
      className={cn(overlayClassName, "fixed inset-0 z-[900] flex justify-end", className)}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={cn(
          panelSurfaceClassName,
          "flex h-full w-[min(90vw,28rem)] flex-col rounded-none border-y-0 border-r-0 border-l-[var(--border-strong)] bg-[var(--card)] shadow-[var(--shadow-lg)]"
        )}
      >
        <NotificationHeader
          unreadCount={unread.length}
          totalCount={notifications.length}
          markAllRead={markAllRead}
          clearAll={clearAll}
          close={toggle}
        />

        <NotificationList
          notifications={notifications}
          markRead={markRead}
          onSelectNotification={handleSelectNotification}
          onApproveNotification={(notification) => {
            void reactToApprovalNotification(notification, "approve");
          }}
          onDenyNotification={(notification) => {
            void reactToApprovalNotification(notification, "deny");
          }}
        />
      </div>
    </div>
  );
}
