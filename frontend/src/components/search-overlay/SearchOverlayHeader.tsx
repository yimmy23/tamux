export function SearchOverlayHeader({
    query,
    matchCount,
    currentIndex,
}: {
    query: string;
    matchCount: number;
    currentIndex: number;
}) {
    return (
        <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "center" }}>
            <div style={{ display: "grid", gap: 2 }}>
                <span className="amux-panel-title" style={{ color: "var(--mission)" }}>Live Search</span>
                <span style={{ fontSize: 12, fontWeight: 700 }}>Buffer Recall</span>
            </div>
            {query ? (
                <span
                    style={{
                        fontSize: 10,
                        color: "var(--text-secondary)",
                        minWidth: 40,
                        textAlign: "center",
                    }}
                >
                    {matchCount > 0 ? `${currentIndex + 1}/${matchCount}` : "0/0"}
                </span>
            ) : null}
        </div>
    );
}
