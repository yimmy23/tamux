import { useAgentStore } from "../lib/agentStore";
import { cn, panelSurfaceClassName } from "./ui/shared";

export function ConciergeToast() {
    const welcome = useAgentStore((s) => s.conciergeWelcome);
    const dismiss = useAgentStore((s) => s.dismissConciergeWelcome);
    const config = useAgentStore((s) => s.conciergeConfig);
    const setActiveThread = useAgentStore((s) => s.setActiveThread);
    const createThread = useAgentStore((s) => s.createThread);

    if (!welcome || !config.enabled) return null;

    return (
        <div
            className={cn(panelSurfaceClassName, "backdrop-blur-[var(--panel-blur)]")}
            style={{
                position: "fixed",
                bottom: 20,
                right: 20,
                zIndex: 2147483647,
                maxWidth: 400,
                padding: 16,
                borderColor: "var(--accent-border)",
                background: "color-mix(in srgb, var(--card) 92%, var(--bg-overlay))",
                boxShadow: "var(--shadow-lg)",
            }}
        >
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
                            if (action.action_type === "dismiss") {
                                await dismiss();
                            } else if (action.action_type === "continue_session" && action.thread_id) {
                                setActiveThread(action.thread_id);
                                await dismiss();
                            } else if (action.action_type === "start_new") {
                                createThread({});
                                await dismiss();
                            } else {
                                await dismiss();
                            }
                        }}
                        style={{
                            background: "var(--accent-soft)",
                            border: "1px solid var(--accent-border)",
                            color: "var(--accent)",
                            borderRadius: "var(--radius-sm)",
                            padding: "4px 10px",
                            fontSize: 11,
                            cursor: "pointer",
                            fontFamily: "inherit",
                            transition: "background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast)",
                        }}
                    >
                        {action.label}
                    </button>
                ))}
            </div>
        </div>
    );
}
