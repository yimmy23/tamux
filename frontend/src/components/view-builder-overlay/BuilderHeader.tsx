import { actionButtonStyle } from "./shared";

export function BuilderHeader({
    activeViewId,
    isDirty,
    selectedEditable,
    stopEditing,
}: {
    activeViewId: string | null;
    isDirty: boolean;
    selectedEditable: boolean | null;
    stopEditing: () => void;
}) {
    return (
        <>
            <div style={{ padding: 16, borderBottom: "1px solid rgba(255,255,255,0.08)", display: "flex", justifyContent: "space-between", gap: 12 }}>
                <div>
                    <div style={{ fontSize: 11, letterSpacing: "0.12em", textTransform: "uppercase", color: "var(--text-muted)" }}>
                        Builder Mode
                    </div>
                    <div style={{ marginTop: 4, fontSize: 18, fontWeight: 700 }}>
                        {activeViewId ?? "No active view"}
                    </div>
                    <div style={{ marginTop: 6, fontSize: 12, color: isDirty ? "#ffd166" : "var(--text-muted)" }}>
                        {isDirty ? "Unsaved changes" : "Synced with disk"}
                    </div>
                </div>
                <button
                    onClick={stopEditing}
                    style={{
                        border: "1px solid rgba(255,255,255,0.12)",
                        background: "rgba(255,255,255,0.05)",
                        color: "var(--text-primary)",
                        borderRadius: 10,
                        padding: "8px 12px",
                        cursor: "pointer",
                    }}
                >
                    Exit
                </button>
            </div>

            <section>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 8 }}>
                    <button onClick={() => { void executeCommand("builder.saveView"); }} style={actionButtonStyle("primary")}>Save</button>
                    <button onClick={() => { void executeCommand("builder.discardView"); }} style={actionButtonStyle("secondary")}>Discard</button>
                    <button onClick={() => { void executeCommand("builder.resetView"); }} style={actionButtonStyle("secondary")}>Reset to Default</button>
                    <button onClick={() => { void executeCommand("builder.toggleSelectedEditable"); }} style={actionButtonStyle("secondary")}>
                        {selectedEditable === false ? "Mark Editable" : "Lock Editable"}
                    </button>
                    <button onClick={() => { void executeCommand("builder.moveSelectedUp"); }} style={actionButtonStyle("secondary")}>Move Up</button>
                    <button onClick={() => { void executeCommand("builder.moveSelectedDown"); }} style={actionButtonStyle("secondary")}>Move Down</button>
                    <button onClick={() => { void executeCommand("builder.duplicateSelectedNode"); }} style={actionButtonStyle("secondary")}>Duplicate</button>
                    <button onClick={() => { void executeCommand("builder.deleteSelectedNode"); }} style={actionButtonStyle("secondary")}>Delete</button>
                    <button onClick={() => { void executeCommand("builder.promoteSelectedToBlock"); }} style={actionButtonStyle("secondary")}>Make Block</button>
                </div>
            </section>
        </>
    );
}

import { executeCommand } from "../../registry/commandRegistry";
