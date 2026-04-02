import { useCallback, useEffect, useMemo, useRef } from "react";
import type { MutableRefObject } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { ApprovalRequest } from "@/lib/agentMissionStore";
import { detectInlineApprovalPrompt } from "./inlineApproval";
import { encodeTextToBase64, stripAnsi } from "./utils";

type AddNotification = (entry: {
  title: string;
  body: string;
  subtitle: string | null;
  icon: string;
  source: string;
  workspaceId: string | null;
  surfaceId: string | null;
  paneId: string;
  panelId: string;
}) => void;

export function useTerminalApprovals({
  paneId,
  paneSurfaceId,
  paneWorkspaceId,
  commandBufferRef,
  approvals,
  addNotification,
  clearPaneNotifications,
  clearCanvasPanelStatus,
  markApprovalHandled,
  setCanvasPanelStatus,
}: {
  paneId: string;
  paneSurfaceId?: string;
  paneWorkspaceId?: string;
  commandBufferRef: MutableRefObject<string>;
  approvals: ApprovalRequest[];
  addNotification: AddNotification;
  clearPaneNotifications: (paneId: string, source?: string) => void;
  clearCanvasPanelStatus: (paneId: string) => void;
  markApprovalHandled: (approvalId: string) => void;
  setCanvasPanelStatus: (paneId: string, status: "idle" | "running" | "needs_approval") => void;
}) {
  const setCanvasView = useWorkspaceStore((state) => state.setCanvasView);
  const setCanvasPreviousView = useWorkspaceStore((state) => state.setCanvasPreviousView);
  const pendingApprovalIdRef = useRef<string | null>(null);
  const approvalCommandByIdRef = useRef<Record<string, string>>({});
  const pendingInlineApprovalPromptRef = useRef<{ signature: string; at: number } | null>(null);
  const inlinePromptBufferRef = useRef("");
  const resolvedApprovals = useMemo(
    () => approvals.filter((entry) => entry.paneId === paneId && entry.status !== "pending" && entry.handledAt === null),
    [approvals, paneId],
  );

  const restoreCanvasPreviousView = useCallback(() => {
    const workspaceState = useWorkspaceStore.getState();
    for (const workspace of workspaceState.workspaces) {
      const surface = workspace.surfaces.find((entry) => entry.id === paneSurfaceId);
      if (!surface || surface.layoutMode !== "canvas") {
        continue;
      }
      const previous = surface.canvasState.previousView;
      if (!previous) {
        return;
      }
      setCanvasView(surface.id, previous);
      setCanvasPreviousView(surface.id, null);
      return;
    }
  }, [paneSurfaceId, setCanvasPreviousView, setCanvasView]);

  const clearInlineApprovalPrompt = useCallback(() => {
    if (!pendingInlineApprovalPromptRef.current) return;
    pendingInlineApprovalPromptRef.current = null;
    clearCanvasPanelStatus(paneId);
    clearPaneNotifications(paneId, "approval");
    restoreCanvasPreviousView();
  }, [clearCanvasPanelStatus, clearPaneNotifications, paneId, restoreCanvasPreviousView]);

  const maybeRaiseInlineApprovalPrompt = useCallback((chunkText: string) => {
    if (!chunkText) return;
    const cleaned = stripAnsi(chunkText);
    if (!cleaned) return;

    const nextBuffer = `${inlinePromptBufferRef.current}${cleaned}`.slice(-4096);
    inlinePromptBufferRef.current = nextBuffer;

    const tail = nextBuffer.slice(-512);
    if (!/trust|approv|do you|\?\s*$/i.test(tail)) return;

    const prompt = detectInlineApprovalPrompt(nextBuffer);
    if (!prompt) return;

    const signature = `${paneId}:${prompt.toLowerCase()}`;
    const now = Date.now();
    const pending = pendingInlineApprovalPromptRef.current;
    if (pending && pending.signature === signature && now - pending.at < 6000) {
      return;
    }

    pendingInlineApprovalPromptRef.current = { signature, at: now };
    setCanvasPanelStatus(paneId, "needs_approval");
    clearPaneNotifications(paneId, "approval");
    addNotification({
      title: "Input required",
      body: prompt,
      subtitle: "Agent is waiting for approval",
      icon: "shield",
      source: "approval",
      workspaceId: paneWorkspaceId ?? null,
      surfaceId: paneSurfaceId ?? null,
      paneId,
      panelId: paneId,
    });

    const state = useWorkspaceStore.getState();
    if (!state.notificationPanelOpen) {
      state.toggleNotificationPanel();
    }
  }, [addNotification, clearPaneNotifications, paneId, paneSurfaceId, paneWorkspaceId, setCanvasPanelStatus]);

  useEffect(() => {
    if (resolvedApprovals.length === 0) return;

    const pendingId = pendingApprovalIdRef.current;
    if (!pendingId) return;
    const approval = resolvedApprovals.find((entry) => entry.id === pendingId) ?? resolvedApprovals[0];
    if (!approval) return;

    const amux = getBridge();
    if (approval.status === "approved-once" || approval.status === "approved-session") {
      void amux?.sendTerminalInput?.(paneId, encodeTextToBase64("\r"));
    } else if (approval.status === "denied") {
      commandBufferRef.current = "";
      void amux?.sendTerminalInput?.(paneId, encodeTextToBase64("\u0003"));
    }

    pendingApprovalIdRef.current = null;
    clearCanvasPanelStatus(paneId);
    clearPaneNotifications(paneId, "approval");
    restoreCanvasPreviousView();
    markApprovalHandled(approval.id);
  }, [clearCanvasPanelStatus, clearPaneNotifications, commandBufferRef, markApprovalHandled, paneId, resolvedApprovals, restoreCanvasPreviousView]);

  return {
    pendingApprovalIdRef,
    approvalCommandByIdRef,
    pendingInlineApprovalPromptRef,
    inlinePromptBufferRef,
    restoreCanvasPreviousView,
    clearInlineApprovalPrompt,
    maybeRaiseInlineApprovalPrompt,
  };
}
