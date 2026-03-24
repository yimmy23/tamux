export function StatusIndicator({ label, status }: { label: string; status: "success" | "warning" | "neutral" }) {
    const colors = {
        success: { dot: "var(--success)" },
        warning: { dot: "var(--warning)" },
        neutral: { dot: "var(--text-muted)" },
    }[status];

    return (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-1)" }}>
            <span
                style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: colors.dot,
                }}
            />
            <span>{label}</span>
        </div>
    );
}

export function StatusBadge({
    label,
    tone,
    onClick,
}: {
    label: string;
    tone: "success" | "warning" | "agent" | "neutral";
    onClick?: () => void;
}) {
    const colors = {
        success: { bg: "var(--success-soft)", text: "var(--success)", border: "rgba(74, 222, 128, 0.2)" },
        warning: { bg: "var(--warning-soft)", text: "var(--warning)", border: "rgba(251, 191, 36, 0.2)" },
        agent: { bg: "var(--agent-soft)", text: "var(--agent)", border: "rgba(130, 170, 255, 0.2)" },
        neutral: { bg: "var(--bg-tertiary)", text: "var(--text-muted)", border: "var(--glass-border)" },
    }[tone];

    return (
        <span
            onClick={onClick}
            style={{
                display: "inline-flex",
                alignItems: "center",
                padding: "var(--space-1) var(--space-2)",
                borderRadius: "var(--radius-full)",
                border: `1px solid ${colors.border}`,
                background: colors.bg,
                color: colors.text,
                fontSize: "var(--text-xs)",
                fontWeight: 500,
                cursor: onClick ? "pointer" : "default",
                transition: "all var(--transition-fast)",
            }}
            onMouseEnter={(event) => {
                if (onClick) {
                    event.currentTarget.style.opacity = "0.8";
                }
            }}
            onMouseLeave={(event) => {
                event.currentTarget.style.opacity = "1";
            }}
        >
            {label}
        </span>
    );
}
