import { useEffect, useRef } from "react";
import { AgentApprovalOverlay } from "@/components/AgentApprovalOverlay";
import { AgentChatPanelProvider } from "@/components/agent-chat-panel/runtime";
import { ConciergeToast } from "@/components/ConciergeToast";
import { OperatorProfileOnboardingPanel } from "@/components/OperatorProfileOnboardingPanel";
import { OperatorQuestionOverlay } from "@/components/OperatorQuestionOverlay";
import { SetupOnboardingPanel } from "@/components/SetupOnboardingPanel";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useAuditStore } from "@/lib/auditStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { shouldAutoStartOperatorProfileFromConcierge } from "./conciergeEvents";
import { ZoraiShell } from "./shell/ZoraiShell";

export function ZoraiApp() {
  const operatorProfileAutoStartRequested = useRef(false);

  useEffect(() => {
    useWorkspaceStore.setState({ agentPanelOpen: true });
  }, []);

  useEffect(() => {
    const bridge = getBridge();
    void useNotificationStore.getState().loadSharedNotifications();
    if (!bridge?.onAgentEvent) return;

    const unsubscribe = bridge.onAgentEvent((event: any) => {
      if (event?.type === "concierge_welcome") {
        useAgentStore.setState({
          conciergeWelcome: {
            content: event.content ?? "",
            actions: event.actions ?? [],
          },
        });
        if (
          !operatorProfileAutoStartRequested.current
          && shouldAutoStartOperatorProfileFromConcierge(event)
        ) {
          operatorProfileAutoStartRequested.current = true;
          void useAgentStore.getState().maybeStartOperatorProfileOnboarding();
        }
      }
      if (event?.type === "operator-profile-session-started") {
        useAgentStore.getState().applyOperatorProfileSessionStarted(event.data ?? event);
      }
      if (event?.type === "operator-profile-question") {
        useAgentStore.getState().applyOperatorProfileQuestion(event.data ?? event);
      }
      if (event?.type === "operator-profile-progress") {
        useAgentStore.getState().applyOperatorProfileProgress(event.data ?? event);
      }
      if (event?.type === "operator-profile-session-completed") {
        useAgentStore.getState().applyOperatorProfileSessionCompleted(event.data ?? event);
      }
      if (event?.type === "audit_action") {
        useAuditStore.getState().addEntry({
          id: event.id ?? "",
          timestamp: event.timestamp ?? Date.now(),
          actionType: event.action_type ?? "heartbeat",
          summary: event.summary ?? "",
          explanation: event.explanation ?? null,
          confidence: event.confidence ?? null,
          confidenceBand: event.confidence_band ?? null,
          causalTraceId: event.causal_trace_id ?? null,
          threadId: event.thread_id ?? null,
        });
      }
      if (event?.type === "notification_inbox_upsert" && event.notification) {
        useNotificationStore.getState().upsertSharedNotification(event.notification);
      }
    });

    const timer = window.setTimeout(() => {
      void bridge.agentRequestConciergeWelcome?.().catch(() => {});
    }, 250);

    return () => {
      window.clearTimeout(timer);
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, []);

  return (
    <AgentChatPanelProvider>
      <ZoraiShell />
      <SetupOnboardingPanel />
      <OperatorProfileOnboardingPanel />
      <AgentApprovalOverlay />
      <OperatorQuestionOverlay />
      <ConciergeToast />
    </AgentChatPanelProvider>
  );
}
