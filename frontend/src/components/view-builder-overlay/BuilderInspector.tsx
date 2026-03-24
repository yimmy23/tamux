import { executeCommand } from "../../registry/commandRegistry";
import type { UIViewNode } from "../../schemas/uiSchema";
import { chipButtonStyle, sectionCardStyle, sectionTitleStyle, stringValue, styleValue } from "./shared";

export function BuilderInspector({ selectedNodeDocument }: { selectedNodeDocument: UIViewNode | null }) {
    return (
        <section>
            <div style={sectionTitleStyle}>Inspector</div>
            <div style={{ ...sectionCardStyle, display: "grid", gap: 10 }}>
                {selectedNodeDocument ? (
                    <>
                        <InspectorField
                            label="Title"
                            defaultValue={stringValue(selectedNodeDocument.props?.title)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { title: value }); }}
                        />
                        <InspectorField
                            label="Label"
                            defaultValue={stringValue(selectedNodeDocument.props?.label)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { label: value }); }}
                        />
                        <InspectorField
                            label="Width"
                            defaultValue={stringValue(styleValue(selectedNodeDocument, "width"))}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedStyle", { width: value || undefined }); }}
                        />
                        <InspectorField
                            label="Height"
                            defaultValue={stringValue(styleValue(selectedNodeDocument, "height"))}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedStyle", { height: value || undefined }); }}
                        />
                        <InspectorField
                            label="Min Width"
                            defaultValue={stringValue(selectedNodeDocument.props?.minWidth)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { minWidth: value || undefined }); }}
                        />
                        <InspectorField
                            label="Min Height"
                            defaultValue={stringValue(selectedNodeDocument.props?.minHeight)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { minHeight: value || undefined }); }}
                        />
                        <InspectorField
                            label="Max Width"
                            defaultValue={stringValue(selectedNodeDocument.props?.maxWidth)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { maxWidth: value || undefined }); }}
                        />
                        <InspectorField
                            label="Max Height"
                            defaultValue={stringValue(selectedNodeDocument.props?.maxHeight)}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedProps", { maxHeight: value || undefined }); }}
                        />
                        <BooleanChipField
                            label="Resizable"
                            values={[true, false]}
                            activeValue={selectedNodeDocument.props?.resizable}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedProps", { resizable: value }); }}
                            formatLabel={(value) => value ? "Enabled" : "Disabled"}
                        />
                        <ChipField
                            label="Resize Axis"
                            values={["both", "horizontal", "vertical"] as const}
                            activeValue={selectedNodeDocument.props?.resizeAxis}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedProps", { resizeAxis: value }); }}
                        />
                        <ChipField
                            label="Flex Direction"
                            values={["row", "column"] as const}
                            activeValue={styleValue(selectedNodeDocument, "flexDirection")}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedStyle", { display: "flex", flexDirection: value }); }}
                        />
                        <ChipField
                            label="Justify Content"
                            values={["flex-start", "center", "flex-end", "space-between"] as const}
                            activeValue={styleValue(selectedNodeDocument, "justifyContent")}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedStyle", { display: "flex", justifyContent: value }); }}
                            formatLabel={(value) => value.replace("flex-", "")}
                        />
                        <ChipField
                            label="Align Items"
                            values={["flex-start", "center", "flex-end", "stretch"] as const}
                            activeValue={styleValue(selectedNodeDocument, "alignItems")}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedStyle", { display: "flex", alignItems: value }); }}
                            formatLabel={(value) => value.replace("flex-", "")}
                        />
                        <InspectorField
                            label="Gap"
                            defaultValue={stringValue(styleValue(selectedNodeDocument, "gap"))}
                            onCommit={(value) => { void executeCommand("builder.patchSelectedStyle", { gap: value || undefined }); }}
                        />
                        <ChipField
                            label="Align Self"
                            values={["start", "center", "end", "stretch"] as const}
                            activeValue={styleValue(selectedNodeDocument, "alignSelf")}
                            onSelect={(value) => { void executeCommand("builder.patchSelectedStyle", { alignSelf: value }); }}
                        />
                    </>
                ) : (
                    <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Select a node to edit its properties.</div>
                )}
            </div>
        </section>
    );
}

function InspectorField({
    label,
    defaultValue,
    onCommit,
}: {
    label: string;
    defaultValue: string;
    onCommit: (value: string) => void;
}) {
    return (
        <label style={{ display: "grid", gap: 6 }}>
            <span style={{ fontSize: 12, color: "var(--text-muted)" }}>{label}</span>
            <input
                defaultValue={defaultValue}
                onBlur={(event) => onCommit(event.currentTarget.value)}
                style={{
                    width: "100%",
                    borderRadius: 10,
                    border: "1px solid rgba(255,255,255,0.1)",
                    background: "rgba(255,255,255,0.04)",
                    color: "var(--text-primary)",
                    padding: "8px 10px",
                }}
            />
        </label>
    );
}

function ChipField<T extends string>({
    label,
    values,
    activeValue,
    onSelect,
    formatLabel,
}: {
    label: string;
    values: readonly T[];
    activeValue: unknown;
    onSelect: (value: T) => void;
    formatLabel?: (value: T) => string;
}) {
    return (
        <div style={{ display: "grid", gap: 6 }}>
            <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{label}</div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {values.map((value) => (
                    <button key={value} onClick={() => onSelect(value)} style={chipButtonStyle(activeValue === value)}>
                        {formatLabel ? formatLabel(value) : value}
                    </button>
                ))}
            </div>
        </div>
    );
}

function BooleanChipField({
    label,
    values,
    activeValue,
    onSelect,
    formatLabel,
}: {
    label: string;
    values: readonly boolean[];
    activeValue: unknown;
    onSelect: (value: boolean) => void;
    formatLabel: (value: boolean) => string;
}) {
    return (
        <div style={{ display: "grid", gap: 6 }}>
            <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{label}</div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {values.map((value) => (
                    <button key={String(value)} onClick={() => onSelect(value)} style={chipButtonStyle(activeValue === value)}>
                        {formatLabel(value)}
                    </button>
                ))}
            </div>
        </div>
    );
}
