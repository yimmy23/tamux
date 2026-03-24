export function SharedCursor({ mode }: { mode: "idle" | "human" | "agent" | "approval" }) {
    const palette = {
        idle: { label: "Idle", color: "var(--text-secondary)", glow: "rgba(166, 173, 200, 0.18)" },
        human: { label: "Human", color: "var(--success)", glow: "rgba(166, 227, 161, 0.22)" },
        agent: { label: "Agent", color: "var(--accent)", glow: "rgba(137, 180, 250, 0.22)" },
        approval: { label: "Approval", color: "var(--warning)", glow: "rgba(249, 226, 175, 0.22)" },
    }[mode];

    return (
        <div
            style={{
                position: "absolute",
                top: 14,
                right: 14,
                zIndex: 4,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "6px 10px",
                borderRadius: 0,
                border: "1px solid rgba(255,255,255,0.08)",
                background: "rgba(10, 15, 24, 0.72)",
                backdropFilter: "none",
                boxShadow: "none",
            }}
        >
            <span
                style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: palette.color,
                    boxShadow: "none",
                }}
            />
            <span style={{ fontSize: 11, color: palette.color, letterSpacing: "0.08em", textTransform: "uppercase" }}>
                {palette.label} Cursor
            </span>
        </div>
    );
}