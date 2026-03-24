export function CommandPaletteHeader({ commandCount }: { commandCount: number }) {
    return (
        <div style={{ padding: "var(--space-4)", borderBottom: "1px solid var(--border)" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", marginBottom: "var(--space-2)" }}>
                <span className="amux-agent-indicator" style={{ background: "var(--mission-soft)", borderColor: "var(--mission-glow)", color: "var(--mission)" }}>
                    Action Launcher
                </span>
                <span className="amux-chip">{commandCount} commands</span>
            </div>

            <div style={{ fontSize: "var(--text-xl)", fontWeight: 700 }}>Mission Command Palette</div>
        </div>
    );
}
