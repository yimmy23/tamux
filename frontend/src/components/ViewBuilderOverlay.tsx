import { useMemo } from "react";
import { ComponentRegistryAPI } from "../registry/componentRegistry";
import { VIEW_BUILDER_PRIMITIVE_PALETTE } from "../lib/viewBuilderPrimitives";
import { useViewBuilderStore } from "../lib/viewBuilderStore";
import { BuilderDocumentTree } from "./view-builder-overlay/BuilderDocumentTree";
import { BuilderHeader } from "./view-builder-overlay/BuilderHeader";
import { BuilderInspector } from "./view-builder-overlay/BuilderInspector";
import { BuilderPaletteSection } from "./view-builder-overlay/BuilderPaletteSection";
import { BuilderSelectionPanel } from "./view-builder-overlay/BuilderSelectionPanel";
import { BUILDER_PRIMITIVE_COMPONENTS, findNodeById, findNodeEditable, overlayShellStyle } from "./view-builder-overlay/shared";

export function ViewBuilderOverlay() {
    const isEditMode = useViewBuilderStore((state) => state.isEditMode);
    const activeViewId = useViewBuilderStore((state) => state.activeViewId);
    const selectedNode = useViewBuilderStore((state) => state.selectedNode);
    const selectNode = useViewBuilderStore((state) => state.selectNode);
    const stopEditing = useViewBuilderStore((state) => state.stopEditing);
    const dirtyViewIds = useViewBuilderStore((state) => state.dirtyViewIds);
    const draftDocuments = useViewBuilderStore((state) => state.draftDocuments);

    const draftDocument = activeViewId ? draftDocuments[activeViewId] : null;
    const selectedNodeDocument = useMemo(
        () => (draftDocument && selectedNode?.nodeId ? findNodeById(draftDocument, selectedNode.nodeId) : null),
        [draftDocument, selectedNode?.nodeId],
    );
    const isDirty = activeViewId ? Boolean(dirtyViewIds[activeViewId]) : false;
    const selectedEditable = useMemo(() => {
        if (!draftDocument || !selectedNode?.nodeId) {
            return null;
        }

        return findNodeEditable(draftDocument, selectedNode.nodeId);
    }, [draftDocument, selectedNode?.nodeId]);

    const registeredComponents = useMemo(
        () => ComponentRegistryAPI.list()
            .filter((name) => name !== "Unknown" && !BUILDER_PRIMITIVE_COMPONENTS.has(name))
            .sort((left, right) => left.localeCompare(right)),
        [],
    );

    if (!isEditMode) {
        return null;
    }

    return (
        <aside style={overlayShellStyle}>
            <BuilderHeader
                activeViewId={activeViewId}
                isDirty={isDirty}
                selectedEditable={selectedEditable}
                stopEditing={stopEditing}
            />

            <div style={{ padding: 16, display: "grid", gap: 16 }}>
                <BuilderSelectionPanel
                    nodeId={selectedNode?.nodeId ?? null}
                    componentType={selectedNode?.componentType ?? null}
                    selectedEditable={selectedEditable}
                />

                <BuilderInspector selectedNodeDocument={selectedNodeDocument} />

                <BuilderDocumentTree
                    activeViewId={activeViewId}
                    draftDocument={draftDocument}
                    selectedNodeId={selectedNode?.nodeId ?? null}
                    onSelect={selectNode}
                />

                <BuilderPaletteSection
                    title="Primitive Palette"
                    items={VIEW_BUILDER_PRIMITIVE_PALETTE.map((item) => ({
                        key: item.id,
                        label: item.label,
                        payload: { blockId: item.blockId, componentType: item.componentType },
                    }))}
                />

                <BuilderPaletteSection
                    title="Runtime Components"
                    items={registeredComponents.map((name) => ({
                        key: name,
                        label: name,
                        payload: { componentType: name },
                    }))}
                />

                <section>
                    <div style={{ fontSize: 12, fontWeight: 700, marginBottom: 8, color: "var(--text-secondary)" }}>
                        Next Interaction Targets
                    </div>
                    <div style={{ fontSize: 13, lineHeight: 1.6, color: "var(--text-secondary)" }}>
                        This first builder slice supports edit mode entry, node targeting, and a live component palette.
                        Drag, resize, align, and YAML mutation can now build on stable node ids instead of anonymous tree positions.
                    </div>
                </section>
            </div>
        </aside>
    );
}
