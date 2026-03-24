export function TerminalPaneHeader({
    paneId,
    paneName,
    paneNameDraft,
    setPaneNameDraft,
    setPaneName,
}: {
    paneId: string;
    paneName: string;
    paneNameDraft: string;
    setPaneNameDraft: (value: string) => void;
    setPaneName: (paneId: string, name: string) => void;
}) {
    return (
        <div
            style={{
                height: 28,
                maxWidth: 300,
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 8,
                marginBottom: 8,
                padding: "0 2px",
                color: "var(--text-secondary)",
                fontSize: 11,
            }}
        >
            <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0, flex: 1 }}>
                <span style={{ color: "var(--text-muted)", whiteSpace: "nowrap" }}>Pane</span>
                <input
                    value={paneNameDraft}
                    onChange={(event) => setPaneNameDraft(event.target.value)}
                    onMouseDown={(event) => event.stopPropagation()}
                    onClick={(event) => event.stopPropagation()}
                    onBlur={() => setPaneName(paneId, paneNameDraft || paneName)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            event.preventDefault();
                            setPaneName(paneId, paneNameDraft || paneName);
                            (event.currentTarget as HTMLInputElement).blur();
                        } else if (event.key === "Escape") {
                            event.preventDefault();
                            setPaneNameDraft(paneName);
                            (event.currentTarget as HTMLInputElement).blur();
                        }
                    }}
                    style={{
                        flex: 1,
                        minWidth: 80,
                        maxWidth: 240,
                        height: 22,
                        border: "1px solid var(--border)",
                        background: "var(--bg-secondary)",
                        color: "var(--text-primary)",
                        padding: "0 6px",
                        fontSize: 11,
                        fontFamily: "var(--font-mono)",
                    }}
                />
            </div>
            <span style={{ color: "var(--text-muted)", fontFamily: "var(--font-mono)", whiteSpace: "nowrap" }}>
                {paneId}
            </span>
        </div>
    );
}