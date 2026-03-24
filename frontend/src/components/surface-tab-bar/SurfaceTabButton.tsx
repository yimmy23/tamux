import type { ReactNode } from "react";
import { actionButtonBaseStyle } from "./shared";

export function ActionButton({
    title,
    onClick,
    children,
}: {
    title: string;
    onClick: () => void;
    children: ReactNode;
}) {
    return (
        <button
            onClick={onClick}
            title={title}
            style={actionButtonBaseStyle}
            onMouseEnter={(event) => {
                event.currentTarget.style.background = "var(--bg-tertiary)";
                event.currentTarget.style.color = "var(--text-primary)";
            }}
            onMouseLeave={(event) => {
                event.currentTarget.style.background = "transparent";
                event.currentTarget.style.color = "var(--text-muted)";
            }}
        >
            {children}
        </button>
    );
}
