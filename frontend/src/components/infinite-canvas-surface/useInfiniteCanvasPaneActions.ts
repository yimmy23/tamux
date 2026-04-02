import { useCallback, useMemo } from "react";
import { allLeafIds, findLeaf } from "../../lib/bspTree";
import { useAgentMissionStore } from "../../lib/agentMissionStore";
import {
  cloneSessionForDuplication,
  queuePaneBootstrapCommand,
  resolveDuplicateActiveBootstrapCommand,
  resolveDuplicateBootstrapCommand,
  resolveDuplicateSourceSessionId,
} from "../../lib/paneDuplication";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import type { Surface } from "../../lib/types";
import type { CanvasPanelRecord } from "./types";

type UseInfiniteCanvasPaneActionsOptions = {
  surface: Surface;
  panelByPane: Map<string, CanvasPanelRecord>;
  selectedPaneIds: string[];
  selectedPaneSet: Set<string>;
  workspaces: ReturnType<typeof useWorkspaceStore.getState>["workspaces"];
  setSelectedPaneIds: React.Dispatch<React.SetStateAction<string[]>>;
  setContextMenu: React.Dispatch<React.SetStateAction<{ x: number; y: number; paneId: string } | null>>;
};

export function useInfiniteCanvasPaneActions({
  surface,
  panelByPane,
  selectedPaneIds,
  selectedPaneSet,
  workspaces,
  setSelectedPaneIds,
  setContextMenu,
}: UseInfiniteCanvasPaneActionsOptions) {
  const createCanvasPanel = useWorkspaceStore((state) => state.createCanvasPanel);
  const createSurface = useWorkspaceStore((state) => state.createSurface);
  const renameSurface = useWorkspaceStore((state) => state.renameSurface);
  const closePane = useWorkspaceStore((state) => state.closePane);
  const splitActive = useWorkspaceStore((state) => state.splitActive);
  const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
  const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const setPaneSessionId = useWorkspaceStore((state) => state.setPaneSessionId);
  const setPaneIcon = useWorkspaceStore((state) => state.setPaneIcon);
  const setPaneName = useWorkspaceStore((state) => state.setPaneName);
  const moveCanvasPanel = useWorkspaceStore((state) => state.moveCanvasPanel);
  const resizeCanvasPanel = useWorkspaceStore((state) => state.resizeCanvasPanel);
  const operationalEvents = useAgentMissionStore((state) => state.operationalEvents);

  const resolveContextPaneIds = useCallback((paneId: string) => {
    if (selectedPaneSet.has(paneId) && selectedPaneIds.length > 0) {
      return selectedPaneIds;
    }
    return [paneId];
  }, [selectedPaneIds, selectedPaneSet]);

  const requestClosePanes = useCallback((paneId: string) => {
    return resolveContextPaneIds(paneId);
  }, [resolveContextPaneIds]);

  const runSnippetForPanes = useCallback((paneIdsToRun: string[]) => {
    if (paneIdsToRun.length === 0) return;
    setActivePaneId(paneIdsToRun[0]);
    const state = useWorkspaceStore.getState();
    if (!state.snippetPickerOpen) {
      state.toggleSnippetPicker();
    }
  }, [setActivePaneId]);

  const movePaneIdsToSurface = useCallback((paneIdsToMove: string[], targetWorkspaceId: string, targetSurfaceId: string) => {
    if (paneIdsToMove.length === 0) return;
    let targetSurfaceSeedConsumed = false;

    for (let index = 0; index < paneIdsToMove.length; index += 1) {
      const sourcePaneId = paneIdsToMove[index];
      const sourcePanel = panelByPane.get(sourcePaneId);
      const sourceSessionId = sourcePanel?.sessionId ?? findLeaf(surface.layout, sourcePaneId)?.sessionId ?? null;
      const sourceName = surface.paneNames[sourcePaneId] ?? sourcePaneId;
      const sourceIcon = surface.paneIcons[sourcePaneId] ?? "terminal";

      setActiveWorkspace(targetWorkspaceId);
      setActiveSurface(targetSurfaceId);

      const targetWorkspace = useWorkspaceStore.getState().workspaces.find((entry) => entry.id === targetWorkspaceId);
      const targetSurface = targetWorkspace?.surfaces.find((entry) => entry.id === targetSurfaceId);
      if (!targetSurface) continue;

      let newPaneId: string | null = null;
      if (targetSurface.layoutMode === "canvas") {
        newPaneId = createCanvasPanel(targetSurface.id, {
          paneName: sourceName,
          paneIcon: sourceIcon,
          sessionId: sourceSessionId,
        });
      } else {
        const targetPaneIds = allLeafIds(targetSurface.layout);
        const activeTargetPaneId = targetSurface.activePaneId ?? targetPaneIds[0] ?? null;
        const canReuseSeedPane = !targetSurfaceSeedConsumed && targetPaneIds.length === 1 && activeTargetPaneId;
        if (canReuseSeedPane && index === 0) {
          newPaneId = activeTargetPaneId;
          targetSurfaceSeedConsumed = true;
        } else {
          splitActive("horizontal", sourceName, {
            sessionId: sourceSessionId,
            paneIcon: sourceIcon,
          });
          newPaneId = useWorkspaceStore.getState().activePaneId();
          targetSurfaceSeedConsumed = true;
        }
      }

      if (!newPaneId) continue;
      setPaneName(newPaneId, sourceName);
      setPaneIcon(newPaneId, sourceIcon);
      if (sourceSessionId) {
        setPaneSessionId(newPaneId, sourceSessionId);
      }
      if (targetSurface.layoutMode === "canvas" && sourcePanel) {
        resizeCanvasPanel(newPaneId, sourcePanel.width, sourcePanel.height);
        const offset = 24 * (index + 1);
        moveCanvasPanel(newPaneId, sourcePanel.x + offset, sourcePanel.y + offset);
      }

      setActiveWorkspace(surface.workspaceId);
      setActiveSurface(surface.id);
      closePane(sourcePaneId, { stopSession: false, captureTranscript: false });
    }

    setSelectedPaneIds([]);
    setContextMenu(null);
  }, [
    closePane,
    createCanvasPanel,
    moveCanvasPanel,
    panelByPane,
    resizeCanvasPanel,
    setContextMenu,
    setActiveSurface,
    setActiveWorkspace,
    setPaneIcon,
    setPaneName,
    setPaneSessionId,
    setSelectedPaneIds,
    splitActive,
    surface.id,
    surface.layout,
    surface.paneIcons,
    surface.paneNames,
    surface.workspaceId,
  ]);

  const movePaneIdsToWorkspace = useCallback((paneIdsToMove: string[], workspaceId: string) => {
    const targetWorkspace = workspaces.find((entry) => entry.id === workspaceId);
    const targetSurface = targetWorkspace?.surfaces.find((entry) => entry.id === targetWorkspace.activeSurfaceId)
      ?? targetWorkspace?.surfaces[0];
    if (!targetSurface) return;
    movePaneIdsToSurface(paneIdsToMove, workspaceId, targetSurface.id);
  }, [movePaneIdsToSurface, workspaces]);

  const convertPaneIdsToBsp = useCallback((paneIdsToConvert: string[]) => {
    createSurface(surface.workspaceId, { layoutMode: "bsp" });
    const workspace = useWorkspaceStore.getState().workspaces.find((entry) => entry.id === surface.workspaceId);
    const targetSurfaceId = workspace?.activeSurfaceId;
    if (!targetSurfaceId) return;
    renameSurface(targetSurfaceId, "Converted from Canvas");
    movePaneIdsToSurface(paneIdsToConvert, surface.workspaceId, targetSurfaceId);
  }, [createSurface, movePaneIdsToSurface, renameSurface, surface.workspaceId]);

  const duplicatePaneIds = useCallback(async (paneIdsToDuplicate: string[]) => {
    if (paneIdsToDuplicate.length === 0) return;
    const createdPaneIds: string[] = [];

    for (let index = 0; index < paneIdsToDuplicate.length; index += 1) {
      const sourcePaneId = paneIdsToDuplicate[index];
      const sourcePanel = panelByPane.get(sourcePaneId);
      const sourceSessionId = resolveDuplicateSourceSessionId(
        sourcePaneId,
        sourcePanel?.sessionId ?? findLeaf(surface.layout, sourcePaneId)?.sessionId ?? null,
        operationalEvents,
      );
      const sourceWorkspace = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === surface.workspaceId);
      const cloneResult = await cloneSessionForDuplication(sourcePaneId, sourceSessionId, {
        workspaceId: surface.workspaceId,
        cwd: sourceWorkspace?.cwd || null,
      });
      setActivePaneId(sourcePaneId);
      const duplicatedPaneId = createCanvasPanel(surface.id, {
        paneName: `${surface.paneNames[sourcePaneId] ?? sourcePaneId} Copy`,
        paneIcon: surface.paneIcons[sourcePaneId] ?? "terminal",
        sessionId: cloneResult?.sessionId ?? null,
        ...(sourcePanel
          ? {
            width: sourcePanel.width,
            height: sourcePanel.height,
            x: sourcePanel.x + 28 * (index + 1),
            y: sourcePanel.y + 28 * (index + 1),
          }
          : {}),
      });
      if (!duplicatedPaneId) continue;

      const bootstrapCommand =
        resolveDuplicateActiveBootstrapCommand(sourcePaneId, operationalEvents)
        ?? resolveDuplicateBootstrapCommand(sourcePaneId, operationalEvents)
        ?? cloneResult?.activeCommand;
      if (bootstrapCommand) {
        queuePaneBootstrapCommand(duplicatedPaneId, bootstrapCommand);
      }

      createdPaneIds.push(duplicatedPaneId);
    }

    if (createdPaneIds.length > 0) {
      setSelectedPaneIds(createdPaneIds);
      setActivePaneId(createdPaneIds[0]);
    }
  }, [
    createCanvasPanel,
    operationalEvents,
    panelByPane,
    setActivePaneId,
    setSelectedPaneIds,
    surface.id,
    surface.layout,
    surface.paneIcons,
    surface.paneNames,
    surface.workspaceId,
  ]);

  const workspaceMoveTargets = useMemo(
    () => workspaces.filter((workspace) => workspace.id !== surface.workspaceId),
    [surface.workspaceId, workspaces],
  );

  return {
    duplicatePaneIds,
    movePaneIdsToSurface,
    movePaneIdsToWorkspace,
    convertPaneIdsToBsp,
    requestClosePanes,
    resolveContextPaneIds,
    runSnippetForPanes,
    workspaceMoveTargets,
  };
}
