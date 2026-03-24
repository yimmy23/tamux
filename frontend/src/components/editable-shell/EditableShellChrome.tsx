export function EditableShellChrome({
    menuOpen,
    onEdit,
}: {
    menuOpen: boolean;
    onEdit: () => void;
}) {
    return (
        <div style={{ position: "absolute", top: 8, right: 8, zIndex: 40, display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 6 }}>
            {menuOpen ? (
                <div
                    style={{
                        minWidth: 140,
                        borderRadius: 12,
                        border: "1px solid rgba(255,255,255,0.12)",
                        background: "rgba(10, 14, 24, 0.96)",
                        boxShadow: "0 18px 40px rgba(0,0,0,0.35)",
                        overflow: "hidden",
                    }}
                >
                    <button
                        type="button"
                        onClick={(event) => {
                            event.stopPropagation();
                            onEdit();
                        }}
                        style={{
                            width: "100%",
                            padding: "10px 12px",
                            textAlign: "left",
                            border: 0,
                            background: "transparent",
                            color: "var(--text-primary)",
                            cursor: "pointer",
                        }}
                    >
                        Edit
                    </button>
                </div>
            ) : null}
        </div>
    );
}