import { useCallback } from "react";
import { allLeafIds } from "../../lib/bspTree";
import type { OperationalEvent } from "../../lib/agentMissionStore";
import {
  cloneSessionForDuplication,
  queuePaneBootstrapCommand,
  resolveDuplicateActiveBootstrapCommand,
  resolveDuplicateBootstrapCommand,
  resolveDuplicateSourceSessionId,
} from "../../lib/paneDuplication";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import type {
  ConfirmDialogState,
  IconPickerState,
  PaneMeta,
  SurfaceMeta,
  TreeContextMenu,
} from "./sidebarTypes";

type UseSidebarOperationsParams = {
  workspaces: ReturnType<typeof useWorkspaceStore.getState>["workspaces"];
  paneMetaById: Map<string, PaneMeta>;
  surfaceMetaById: Map<string, SurfaceMeta>;
  operationalEvents: OperationalEvent[];
  contextMenu: TreeContextMenu | null;
  setContextMenu: React.Dispatch<React.SetStateAction<TreeContextMenu | null>>;
  setIconPicker: React.Dispatch<React.SetStateAction<IconPickerState | null>>;
  setConfirmDialog: React.Dispatch<React.SetStateAction<ConfirmDialogState | null>>;
  setEditingWorkspaceId: React.Dispatch<React.SetStateAction<string | null>>;
  setWorkspaceNameDraft: React.Dispatch<React.SetStateAction<string>>;
  setEditingPaneId: React.Dispatch<React.SetStateAction<string | null>>;
  setPaneNameDraft: React.Dispatch<React.SetStateAction<string>>;
  clearSelections: () => void;
  resolvePaneContextSelection: (workspaceId: string, paneId: string) => string[];
  resolveSurfaceContextSelection: (workspaceId: string, surfaceId: string) => string[];
  setSelectionWorkspaceId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedPaneIds: React.Dispatch<React.SetStateAction<string[]>>;
  setSelectedSurfaceIds: React.Dispatch<React.SetStateAction<string[]>>;
  setPaneSelectionAnchor: React.Dispatch<React.SetStateAction<string | null>>;
  setSurfaceSelectionAnchor: React.Dispatch<React.SetStateAction<string | null>>;
  setActiveWorkspace: (workspaceId: string) => void;
  createSurface: (
    workspaceId: string,
    options?: { layoutMode?: "bsp" | "canvas" },
  ) => void;
  closeWorkspace: (workspaceId: string) => void;
  closeSurface: (surfaceId: string) => void;
  closePane: (paneId: string) => void;
  createCanvasPanel: (
    surfaceId: string,
    options?: {
      paneName?: string;
      paneIcon?: string;
      sessionId?: string | null;
      width?: number;
      height?: number;
      x?: number;
      y?: number;
    },
  ) => string | null | undefined;
  setActiveSurface: (surfaceId: string) => void;
  setActivePaneId: (paneId: string) => void;
  splitActive: (
    direction: "horizontal" | "vertical",
    paneName?: string,
    options?: { paneIcon?: string; sessionId?: string | null },
  ) => void;
};

export function useSidebarOperations({
  workspaces,
  paneMetaById,
  surfaceMetaById,
  operationalEvents,
  contextMenu,
  setContextMenu,
  setIconPicker,
  setConfirmDialog,
  setEditingWorkspaceId,
  setWorkspaceNameDraft,
  setEditingPaneId,
  setPaneNameDraft,
  clearSelections,
  resolvePaneContextSelection,
  resolveSurfaceContextSelection,
  setSelectionWorkspaceId,
  setSelectedPaneIds,
  setSelectedSurfaceIds,
  setPaneSelectionAnchor,
  setSurfaceSelectionAnchor,
  setActiveWorkspace,
  createSurface,
  closeWorkspace,
  closeSurface,
  closePane,
  createCanvasPanel,
  setActiveSurface,
  setActivePaneId,
  splitActive,
}: UseSidebarOperationsParams) {
  const showConfirm = (state: ConfirmDialogState) => {
    setConfirmDialog(state);
  };

  const appendTerminalToSurface = (
    workspaceId: string,
    surfaceId: string,
    paneId?: string,
  ) => {
    setActiveWorkspace(workspaceId);
    setActiveSurface(surfaceId);
    if (paneId) {
      setActivePaneId(paneId);
    }

    const state = useWorkspaceStore.getState();
    const workspace = state.workspaces.find((entry) => entry.id === workspaceId);
    const surface = workspace?.surfaces.find((entry) => entry.id === surfaceId);
    if (!surface) return;

    if (surface.layoutMode === "canvas") {
      state.createCanvasPanel(surface.id);
    } else {
      state.splitActive("horizontal", "New Terminal");
    }
  };

  const appendTerminalToWorkspace = (workspaceId: string) => {
    const state = useWorkspaceStore.getState();
    const workspace = state.workspaces.find((entry) => entry.id === workspaceId);
    const targetSurface =
      workspace?.surfaces.find((entry) => entry.id === workspace.activeSurfaceId) ??
      workspace?.surfaces[0];
    if (!targetSurface) return;

    appendTerminalToSurface(
      workspaceId,
      targetSurface.id,
      targetSurface.activePaneId ?? undefined,
    );
  };

  const duplicatePaneIds = useCallback(
    async (paneIds: string[]) => {
      if (paneIds.length === 0) return;
      const createdPaneIds: string[] = [];
      let duplicateWorkspaceId: string | null = null;

      for (let index = 0; index < paneIds.length; index += 1) {
        const paneId = paneIds[index];
        const source = paneMetaById.get(paneId);
        if (!source) continue;

        duplicateWorkspaceId = source.workspaceId;
        setActiveWorkspace(source.workspaceId);
        setActiveSurface(source.surfaceId);
        setActivePaneId(paneId);
        const sourceSessionId = resolveDuplicateSourceSessionId(
          paneId,
          source.sessionId,
          operationalEvents,
        );
        const sourceWorkspace = workspaces.find((w) => w.id === source.workspaceId);
        const cloneResult = await cloneSessionForDuplication(paneId, sourceSessionId, {
          workspaceId: source.workspaceId,
          cwd: sourceWorkspace?.cwd || null,
        });

        if (source.layoutMode === "canvas") {
          const duplicatedPaneId = createCanvasPanel(source.surfaceId, {
            paneName: `${source.paneName} Copy`,
            paneIcon: source.paneIcon,
            sessionId: cloneResult?.sessionId ?? null,
            ...(source.panel
              ? {
                  width: source.panel.width,
                  height: source.panel.height,
                  x: source.panel.x + 28 * (index + 1),
                  y: source.panel.y + 28 * (index + 1),
                }
              : {}),
          });
          if (!duplicatedPaneId) continue;
          const bootstrapCommand =
            resolveDuplicateActiveBootstrapCommand(paneId, operationalEvents) ??
            resolveDuplicateBootstrapCommand(paneId, operationalEvents) ??
            cloneResult?.activeCommand;
          if (bootstrapCommand) {
            queuePaneBootstrapCommand(duplicatedPaneId, bootstrapCommand);
          }
          createdPaneIds.push(duplicatedPaneId);
          continue;
        }

        splitActive("horizontal", `${source.paneName} Copy`, {
          paneIcon: source.paneIcon,
          sessionId: cloneResult?.sessionId ?? null,
        });
        const duplicatedPaneId = useWorkspaceStore.getState().activePaneId();
        if (!duplicatedPaneId) continue;
        const bootstrapCommand =
          resolveDuplicateActiveBootstrapCommand(paneId, operationalEvents) ??
          resolveDuplicateBootstrapCommand(paneId, operationalEvents) ??
          cloneResult?.activeCommand;
        if (bootstrapCommand) {
          queuePaneBootstrapCommand(duplicatedPaneId, bootstrapCommand);
        }
        createdPaneIds.push(duplicatedPaneId);
      }

      if (createdPaneIds.length === 0) return;
      setSelectionWorkspaceId(duplicateWorkspaceId);
      setSelectedSurfaceIds([]);
      setSurfaceSelectionAnchor(null);
      setSelectedPaneIds(createdPaneIds);
      setPaneSelectionAnchor(createdPaneIds[createdPaneIds.length - 1] ?? null);
    },
    [
      createCanvasPanel,
      operationalEvents,
      paneMetaById,
      setActivePaneId,
      setActiveSurface,
      setActiveWorkspace,
      setPaneSelectionAnchor,
      setSelectedPaneIds,
      setSelectedSurfaceIds,
      setSelectionWorkspaceId,
      setSurfaceSelectionAnchor,
      splitActive,
      workspaces,
    ],
  );

  const handleWorkspaceContextAction = (
    action: "rename" | "icon" | "append" | "new-canvas" | "close",
    workspaceId: string,
  ) => {
    setContextMenu(null);
    const workspace = useWorkspaceStore
      .getState()
      .workspaces.find((entry) => entry.id === workspaceId);
    if (!workspace) return;

    if (action === "rename") {
      setEditingWorkspaceId(workspaceId);
      setWorkspaceNameDraft(workspace.name);
      return;
    }
    if (action === "icon") {
      setIconPicker({
        kind: "workspace",
        workspaceId,
        x: contextMenu?.x ?? 120,
        y: contextMenu?.y ?? 120,
      });
      return;
    }
    if (action === "append") {
      appendTerminalToWorkspace(workspaceId);
      return;
    }
    if (action === "new-canvas") {
      createSurface(workspaceId, { layoutMode: "canvas" });
      return;
    }

    showConfirm({
      title: `Close workspace '${workspace.name}'?`,
      message: "This will stop and close all terminals in this workspace.",
      confirmLabel: "Close Workspace",
      tone: "danger",
      action: () => closeWorkspace(workspaceId),
    });
    clearSelections();
  };

  const handleSurfaceContextAction = (
    action: "close",
    workspaceId: string,
    surfaceId: string,
  ) => {
    setContextMenu(null);
    const surfaceIds = resolveSurfaceContextSelection(workspaceId, surfaceId);
    if (action !== "close") return;

    if (surfaceIds.length > 1) {
      showConfirm({
        title: `Close ${surfaceIds.length} surfaces?`,
        message: "This will close all selected surfaces and stop their terminals.",
        confirmLabel: `Close ${surfaceIds.length} Surfaces`,
        tone: "danger",
        action: () => {
          for (const id of surfaceIds) {
            closeSurface(id);
          }
          clearSelections();
        },
      });
      return;
    }

    const surfaceMeta = surfaceMetaById.get(surfaceId);
    showConfirm({
      title: `Close surface '${surfaceMeta?.name ?? surfaceId}'?`,
      message: "This will close the selected surface and stop all of its terminals.",
      confirmLabel: "Close Surface",
      tone: "danger",
      action: () => {
        closeSurface(surfaceId);
        clearSelections();
      },
    });
  };

  const handlePaneContextAction = (
    action: "rename" | "icon" | "append" | "duplicate" | "close",
    workspaceId: string,
    surfaceId: string,
    paneId: string,
  ) => {
    setContextMenu(null);
    const paneIds = resolvePaneContextSelection(workspaceId, paneId);

    const state = useWorkspaceStore.getState();
    const workspace = state.workspaces.find((entry) => entry.id === workspaceId);
    const surface = workspace?.surfaces.find((entry) => entry.id === surfaceId);
    if (!surface) return;

    if (action === "rename" && paneIds.length === 1) {
      setEditingPaneId(paneId);
      setPaneNameDraft(surface.paneNames[paneId] ?? paneId);
      return;
    }
    if (action === "icon") {
      setIconPicker({
        kind: "pane",
        paneIds,
        x: contextMenu?.x ?? 120,
        y: contextMenu?.y ?? 120,
      });
      return;
    }
    if (action === "append" && paneIds.length === 1) {
      appendTerminalToSurface(workspaceId, surfaceId, paneId);
      return;
    }
    if (action === "duplicate") {
      void duplicatePaneIds(paneIds);
      return;
    }

    if (paneIds.length > 1) {
      showConfirm({
        title: `Close ${paneIds.length} terminals?`,
        message: "Terminal output and session state for selected panes will be closed.",
        confirmLabel: `Close ${paneIds.length} Terminals`,
        tone: "danger",
        action: () => {
          for (const id of paneIds) {
            closePane(id);
          }
          clearSelections();
        },
      });
      return;
    }

    const paneLabel = surface.paneNames[paneId] ?? paneId;
    const surfacePaneIds = allLeafIds(surface.layout);
    const shouldCloseWholeSurface =
      surface.layoutMode === "bsp" &&
      surfacePaneIds.length === 1 &&
      (workspace?.surfaces.length ?? 0) > 1;

    if (shouldCloseWholeSurface) {
      showConfirm({
        title: `Close terminal surface '${surface.name}'?`,
        message:
          "This is the last terminal in this BSP surface. The entire surface will be closed.",
        confirmLabel: "Close Surface",
        tone: "danger",
        action: () => {
          closeSurface(surfaceId);
          clearSelections();
        },
      });
      return;
    }

    showConfirm({
      title: `Close terminal '${paneLabel}'?`,
      message: "Terminal output and session state for this pane will be closed.",
      confirmLabel: "Close Terminal",
      tone: "danger",
      action: () => {
        closePane(paneId);
        clearSelections();
      },
    });
  };

  return {
    handleWorkspaceContextAction,
    handleSurfaceContextAction,
    handlePaneContextAction,
  };
}
