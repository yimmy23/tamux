import { hasTerminalController } from "../../lib/terminalRegistry";

export function SearchOverlayStatus({ activePaneId }: { activePaneId: string | null }) {
    return (
        <div
            style={{
                fontSize: 10,
                color: "var(--text-muted)",
                paddingTop: 2,
                letterSpacing: "0.02em",
            }}
        >
            {!activePaneId
                ? "No active terminal pane"
                : !hasTerminalController(activePaneId)
                    ? "Terminal not initialized"
                    : `Searching buffer: ${activePaneId}`}
        </div>
    );
}
