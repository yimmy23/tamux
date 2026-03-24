import { useMemo, useState } from "react";
import type React from "react";
import type { UINodeBuilderMeta } from "../../schemas/uiSchema";
import { useViewBuilderStore } from "../../lib/viewBuilderStore";

interface UseEditableShellStateInput {
    className?: string;
    style?: React.CSSProperties;
    resizable?: boolean;
    minWidth?: number | string;
    minHeight?: number | string;
    maxWidth?: number | string;
    maxHeight?: number | string;
    builderNodeId?: string;
    builderViewId?: string;
    builderComponentType?: string;
    builderMeta?: UINodeBuilderMeta;
}

export function useEditableShellState({
    className,
    style,
    resizable,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderNodeId,
    builderViewId,
    builderComponentType,
    builderMeta,
}: UseEditableShellStateInput) {
    const [menuOpen, setMenuOpen] = useState(false);
    const isEditMode = useViewBuilderStore((state) => state.isEditMode);
    const activeViewId = useViewBuilderStore((state) => state.activeViewId);
    const selectedNode = useViewBuilderStore((state) => state.selectedNode);
    const selectNode = useViewBuilderStore((state) => state.selectNode);
    const startEditing = useViewBuilderStore((state) => state.startEditing);

    const chromeEnabled = Boolean(builderMeta?.editable && builderNodeId && builderViewId && builderComponentType);
    const isSelected = Boolean(
        isEditMode
        && activeViewId === builderViewId
        && selectedNode?.nodeId === builderNodeId,
    );
    const hasWrapperStyling = Boolean(
        className
        || style
        || resizable !== undefined
        || minWidth !== undefined
        || minHeight !== undefined
        || maxWidth !== undefined
        || maxHeight !== undefined,
    );

    const selectionStyle = useMemo<React.CSSProperties | undefined>(() => {
        if (!isSelected) {
            return undefined;
        }

        return {
            boxShadow: "0 0 0 2px rgba(109, 197, 255, 0.85)",
            borderRadius: 12,
        };
    }, [isSelected]);

    const handleSelect = () => {
        if (!isEditMode || !builderNodeId || !builderViewId || !builderComponentType) {
            return;
        }

        selectNode({
            viewId: builderViewId,
            nodeId: builderNodeId,
            componentType: builderComponentType,
        });
    };

    const handleStartEditing = () => {
        if (!builderNodeId || !builderViewId || !builderComponentType) {
            return;
        }

        startEditing({
            viewId: builderViewId,
            nodeId: builderNodeId,
            componentType: builderComponentType,
        });
        setMenuOpen(false);
    };

    return {
        chromeEnabled,
        isSelected,
        hasWrapperStyling,
        selectionStyle,
        menuOpen,
        setMenuOpen,
        isEditMode,
        handleSelect,
        handleStartEditing,
    };
}