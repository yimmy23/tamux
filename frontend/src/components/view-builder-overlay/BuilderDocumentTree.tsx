import { executeCommand } from "../../registry/commandRegistry";
import type { ViewDocument, UIViewNode } from "../../schemas/uiSchema";
import { sectionCardStyle, sectionTitleStyle } from "./shared";

export function BuilderDocumentTree({
    activeViewId,
    draftDocument,
    selectedNodeId,
    onSelect,
}: {
    activeViewId: string | null;
    draftDocument: ViewDocument | null;
    selectedNodeId: string | null;
    onSelect: (selection: { viewId: string; nodeId: string; componentType: string }) => void;
}) {
    return (
        <section>
            <div style={sectionTitleStyle}>Document Tree</div>
            <div style={{ ...sectionCardStyle, padding: 10, display: "grid", gap: 8 }}>
                {draftDocument ? (
                    <>
                        <TreeSectionLabel label="Layout" />
                        <TreeNodeView
                            viewId={activeViewId}
                            node={draftDocument.layout}
                            blocks={draftDocument.blocks}
                            depth={0}
                            selectedNodeId={selectedNodeId}
                            onSelect={onSelect}
                        />
                        {draftDocument.fallback ? (
                            <>
                                <TreeSectionLabel label="Fallback" />
                                <TreeNodeView
                                    viewId={activeViewId}
                                    node={draftDocument.fallback}
                                    blocks={draftDocument.blocks}
                                    depth={0}
                                    selectedNodeId={selectedNodeId}
                                    onSelect={onSelect}
                                />
                            </>
                        ) : null}
                        {Object.entries(draftDocument.blocks ?? {}).map(([key, block]) => (
                            <div key={key} style={{ display: "grid", gap: 6 }}>
                                <TreeSectionLabel label={`Block: ${key}`} />
                                <TreeNodeView
                                    viewId={activeViewId}
                                    node={block.layout}
                                    blocks={draftDocument.blocks}
                                    depth={0}
                                    selectedNodeId={selectedNodeId}
                                    onSelect={onSelect}
                                    expandReferences={false}
                                />
                            </div>
                        ))}
                    </>
                ) : (
                    <div style={{ fontSize: 12, color: "var(--text-muted)" }}>No draft document loaded.</div>
                )}
            </div>
        </section>
    );
}

function TreeSectionLabel({ label }: { label: string }) {
    return (
        <div style={{ fontSize: 11, letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--text-muted)" }}>
            {label}
        </div>
    );
}

function TreeNodeView({
    viewId,
    node,
    blocks,
    depth,
    selectedNodeId,
    onSelect,
    expandReferences = true,
    visitedBlocks = new Set<string>(),
}: {
    viewId: string | null;
    node: UIViewNode;
    blocks?: ViewDocument["blocks"];
    depth: number;
    selectedNodeId: string | null;
    onSelect: (selection: { viewId: string; nodeId: string; componentType: string }) => void;
    expandReferences?: boolean;
    visitedBlocks?: Set<string>;
}) {
    const componentType = node.type ?? node.use ?? "Unknown";
    const nodeId = node.id ?? `${componentType}:${depth}`;
    const isSelected = selectedNodeId === nodeId;
    const referencedBlock = node.use ? blocks?.[node.use] : undefined;
    const canExpandReference = Boolean(node.use && referencedBlock && expandReferences && !visitedBlocks.has(node.use));

    const handlePaletteDrop = (paletteItem: { componentType?: string; blockId?: string }) => {
        void executeCommand("builder.insertChild", {
            targetNodeId: nodeId,
            componentType: paletteItem.componentType,
            blockId: paletteItem.blockId,
        });
    };

    return (
        <div style={{ display: "grid", gap: 4 }}>
            <button
                type="button"
                draggable
                onDragStart={(event) => {
                    event.dataTransfer.setData("text/amux-node-id", nodeId);
                    event.dataTransfer.effectAllowed = "move";
                }}
                onDragOver={(event) => {
                    event.preventDefault();
                    event.dataTransfer.dropEffect = "move";
                }}
                onDrop={(event) => {
                    event.preventDefault();
                    const paletteRaw = event.dataTransfer.getData("text/amux-palette-item");
                    if (paletteRaw) {
                        try {
                            handlePaletteDrop(JSON.parse(paletteRaw) as { componentType?: string; blockId?: string });
                            return;
                        } catch (error) {
                            console.warn("Invalid palette drag payload", error);
                        }
                    }

                    const draggedNodeId = event.dataTransfer.getData("text/amux-node-id");
                    if (!draggedNodeId || draggedNodeId === nodeId) {
                        return;
                    }

                    void executeCommand("builder.moveNodeToTarget", { draggedNodeId, targetNodeId: nodeId });
                }}
                onClick={() => {
                    if (!viewId) {
                        return;
                    }

                    onSelect({ viewId, nodeId, componentType });
                }}
                style={{
                    marginLeft: depth * 14,
                    padding: "7px 10px",
                    borderRadius: 10,
                    border: isSelected ? "1px solid rgba(109, 197, 255, 0.7)" : "1px solid rgba(255,255,255,0.08)",
                    background: isSelected ? "rgba(109, 197, 255, 0.14)" : "rgba(255,255,255,0.03)",
                    color: "var(--text-primary)",
                    textAlign: "left",
                    cursor: "pointer",
                    display: "grid",
                    gap: 2,
                }}
            >
                <span style={{ fontSize: 13, fontWeight: 600 }}>{componentType}</span>
                {referencedBlock ? <span style={{ fontSize: 11, color: "#9bd1ff" }}>uses {node.use}</span> : null}
                <span style={{ fontSize: 11, color: "var(--text-muted)" }}>{nodeId}</span>
            </button>
            {canExpandReference && referencedBlock ? (
                <div style={{ display: "grid", gap: 4 }}>
                    <div style={{ marginLeft: (depth + 1) * 14, fontSize: 10, letterSpacing: "0.06em", textTransform: "uppercase", color: "#9bd1ff" }}>
                        Referenced Block
                    </div>
                    <TreeNodeView
                        viewId={viewId}
                        node={referencedBlock.layout}
                        blocks={blocks}
                        depth={depth + 1}
                        selectedNodeId={selectedNodeId}
                        onSelect={onSelect}
                        visitedBlocks={new Set([...visitedBlocks, node.use as string])}
                    />
                </div>
            ) : null}
            {node.children?.map((child) => (
                <TreeNodeView
                    key={child.id ?? `${componentType}-${depth}`}
                    viewId={viewId}
                    node={child}
                    blocks={blocks}
                    depth={depth + 1}
                    selectedNodeId={selectedNodeId}
                    onSelect={onSelect}
                    visitedBlocks={visitedBlocks}
                />
            ))}
        </div>
    );
}
