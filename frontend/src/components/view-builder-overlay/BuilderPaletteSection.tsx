import { executeCommand } from "../../registry/commandRegistry";
import { sectionTitleStyle } from "./shared";

export function BuilderPaletteSection({
    title,
    items,
    buildPayload,
}: {
    title: string;
    items: Array<{ key: string; label: string; payload: { componentType?: string; blockId?: string } }>;
    buildPayload?: (item: { key: string; label: string; payload: { componentType?: string; blockId?: string } }) => { componentType?: string; blockId?: string };
}) {
    return (
        <section>
            <div style={sectionTitleStyle}>{title}</div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                {items.map((item) => {
                    const payload = buildPayload ? buildPayload(item) : item.payload;
                    return (
                        <button
                            key={item.key}
                            type="button"
                            draggable
                            onDragStart={(event) => {
                                event.dataTransfer.setData("text/amux-palette-item", JSON.stringify(payload));
                                event.dataTransfer.effectAllowed = "copyMove";
                            }}
                            onClick={() => { void executeCommand("builder.insertChild", payload); }}
                            style={{
                                fontSize: 12,
                                padding: "6px 10px",
                                borderRadius: 999,
                                background: "rgba(255,255,255,0.06)",
                                border: "1px solid rgba(255,255,255,0.08)",
                                color: "var(--text-primary)",
                                cursor: "pointer",
                            }}
                        >
                            {item.label}
                        </button>
                    );
                })}
            </div>
        </section>
    );
}
