import type { ReactNode } from "react";

export function Overlay({ onClose, children }: { onClose: () => void; children: ReactNode }) {
    return (
        <div
            onClick={onClose}
            style={{
                position: "fixed",
                inset: 0,
                background: "rgba(0,0,0,0.5)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                zIndex: 960,
            }}
        >
            <div onClick={(event) => event.stopPropagation()}>{children}</div>
        </div>
    );
}
