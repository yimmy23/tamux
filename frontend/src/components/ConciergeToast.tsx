import { useAgentStore } from "../lib/agentStore";
import { useWorkspaceStore } from "../lib/workspaceStore";

export function ConciergeToast() {
    const welcome = useAgentStore((s) => s.conciergeWelcome);
    const dismiss = useAgentStore((s) => s.dismissConciergeWelcome);
    const config = useAgentStore((s) => s.conciergeConfig);
    const threads = useAgentStore((s) => s.threads);
    const setActiveThread = useAgentStore((s) => s.setActiveThread);
    const createThread = useAgentStore((s) => s.createThread);
    const settingsOpen = useWorkspaceStore((s) => s.settingsOpen);
    const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);

    if (!welcome || !config.enabled) return null;

    const resolveLocalThreadId = (threadId?: string) => {
        if (!threadId) return null;
        const byLocalId = threads.find((thread) => thread.id === threadId);
        if (byLocalId) return byLocalId.id;
        const byDaemonId = threads.find((thread) => thread.daemonThreadId === threadId);
        return byDaemonId?.id ?? null;
    };

    return (
        <div style={{
            position: "fixed",
            bottom: 20,
            right: 20,
            zIndex: 2147483647,
            maxWidth: 400,
            background: "rgba(18, 33, 47, 0.95)",
            border: "1px solid var(--accent)",
            borderRadius: 8,
            padding: 16,
            boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
        }}>
            <div style={{ fontSize: 10, color: "var(--accent)", fontWeight: 700, marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                Concierge
            </div>
            <div style={{ fontSize: 12, color: "var(--text-primary)", lineHeight: 1.5, whiteSpace: "pre-wrap", marginBottom: 10 }}>
                {welcome.content}
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {welcome.actions.map((action, i) => (
                    <button
                        key={i}
                        onClick={async () => {
                            if (action.action_type === "dismiss" || action.action_type === "dismiss_welcome") {
                                await dismiss();
                            } else if (action.action_type === "continue_session" && action.thread_id) {
                                const targetThreadId = resolveLocalThreadId(action.thread_id);
                                if (targetThreadId) {
                                    setActiveThread(targetThreadId);
                                }
                                await dismiss();
                            } else if (action.action_type === "start_new") {
                                createThread({});
                                await dismiss();
                            } else if (action.action_type === "start_goal_run") {
                                // Navigate to goal run creation
                                createThread({});
                                await dismiss();
                            } else if (action.action_type === "focus_chat") {
                                // Focus chat input
                                const conciergeThreadId = resolveLocalThreadId("concierge");
                                if (conciergeThreadId) {
                                    setActiveThread(conciergeThreadId);
                                }
                                await dismiss();
                            } else if (action.action_type === "open_settings") {
                                // Navigate to settings panel
                                if (!settingsOpen) toggleSettings();
                                await dismiss();
                            } else {
                                await dismiss();
                            }
                        }}
                        style={{
                            background: "rgba(97, 197, 255, 0.1)",
                            border: "1px solid rgba(97, 197, 255, 0.3)",
                            color: "var(--accent)",
                            borderRadius: 4,
                            padding: "4px 10px",
                            fontSize: 11,
                            cursor: "pointer",
                            fontFamily: "inherit",
                        }}
                    >
                        {action.label}
                    </button>
                ))}
                {welcome.actions.length === 0 && (
                    <button
                        onClick={() => dismiss()}
                        style={{
                            background: "rgba(97, 197, 255, 0.1)",
                            border: "1px solid rgba(97, 197, 255, 0.3)",
                            color: "var(--accent)",
                            borderRadius: 4,
                            padding: "4px 10px",
                            fontSize: 11,
                            cursor: "pointer",
                            fontFamily: "inherit",
                        }}
                    >
                        Dismiss
                    </button>
                )}
            </div>
        </div>
    );
}
