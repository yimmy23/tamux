import { useCallback, useEffect, useState } from "react";
import type { PaneMeta, SurfaceMeta } from "./sidebarTypes";

export function useSidebarSelection(
  paneMetaById: Map<string, PaneMeta>,
  surfaceMetaById: Map<string, SurfaceMeta>,
  paneOrderByWorkspace: Map<string, string[]>,
  surfaceOrderByWorkspace: Map<string, string[]>,
) {
  const [selectionWorkspaceId, setSelectionWorkspaceId] = useState<string | null>(null);
  const [selectedPaneIds, setSelectedPaneIds] = useState<string[]>([]);
  const [selectedSurfaceIds, setSelectedSurfaceIds] = useState<string[]>([]);
  const [paneSelectionAnchor, setPaneSelectionAnchor] = useState<string | null>(null);
  const [surfaceSelectionAnchor, setSurfaceSelectionAnchor] = useState<string | null>(null);

  useEffect(() => {
    const validPaneIds = new Set(paneMetaById.keys());
    const validSurfaceIds = new Set(surfaceMetaById.keys());
    setSelectedPaneIds((current) => current.filter((paneId) => validPaneIds.has(paneId)));
    setSelectedSurfaceIds((current) =>
      current.filter((surfaceId) => validSurfaceIds.has(surfaceId)),
    );
    setPaneSelectionAnchor((current) =>
      current && validPaneIds.has(current) ? current : null,
    );
    setSurfaceSelectionAnchor((current) =>
      current && validSurfaceIds.has(current) ? current : null,
    );
  }, [paneMetaById, surfaceMetaById]);

  const clearSelections = useCallback(() => {
    setSelectedPaneIds([]);
    setSelectedSurfaceIds([]);
    setSelectionWorkspaceId(null);
    setPaneSelectionAnchor(null);
    setSurfaceSelectionAnchor(null);
  }, []);

  const selectPaneInWorkspace = useCallback(
    (
      workspaceId: string,
      paneId: string,
      opts?: { toggle?: boolean; range?: boolean; preserveIfAlreadySelected?: boolean },
    ) => {
      const inWorkspace = selectionWorkspaceId === workspaceId;
      const current = inWorkspace ? selectedPaneIds : [];
      const order = paneOrderByWorkspace.get(workspaceId) ?? [];
      const ordered = order.length > 0 ? order : [paneId];
      const toggle = Boolean(opts?.toggle);
      const range = Boolean(opts?.range);

      setSelectedSurfaceIds([]);
      setSurfaceSelectionAnchor(null);
      setSelectionWorkspaceId(workspaceId);

      if (range) {
        const anchor =
          paneSelectionAnchor && ordered.includes(paneSelectionAnchor)
            ? paneSelectionAnchor
            : paneId;
        const from = ordered.indexOf(anchor);
        const to = ordered.indexOf(paneId);
        if (from >= 0 && to >= 0) {
          const [start, end] = from < to ? [from, to] : [to, from];
          setSelectedPaneIds(ordered.slice(start, end + 1));
        } else {
          setSelectedPaneIds([paneId]);
        }
        setPaneSelectionAnchor(anchor);
        return;
      }

      if (toggle) {
        if (current.includes(paneId)) {
          const next = current.filter((id) => id !== paneId);
          setSelectedPaneIds(next);
          setPaneSelectionAnchor(next.length > 0 ? paneId : null);
        } else {
          setSelectedPaneIds([...current, paneId]);
          setPaneSelectionAnchor(paneId);
        }
        return;
      }

      if (
        opts?.preserveIfAlreadySelected &&
        current.includes(paneId) &&
        current.length > 1
      ) {
        return;
      }

      setSelectedPaneIds([paneId]);
      setPaneSelectionAnchor(paneId);
    },
    [
      paneOrderByWorkspace,
      paneSelectionAnchor,
      selectedPaneIds,
      selectionWorkspaceId,
    ],
  );

  const selectSurfaceInWorkspace = useCallback(
    (
      workspaceId: string,
      surfaceId: string,
      opts?: { toggle?: boolean; range?: boolean; preserveIfAlreadySelected?: boolean },
    ) => {
      const inWorkspace = selectionWorkspaceId === workspaceId;
      const current = inWorkspace ? selectedSurfaceIds : [];
      const order = surfaceOrderByWorkspace.get(workspaceId) ?? [];
      const ordered = order.length > 0 ? order : [surfaceId];
      const toggle = Boolean(opts?.toggle);
      const range = Boolean(opts?.range);

      setSelectedPaneIds([]);
      setPaneSelectionAnchor(null);
      setSelectionWorkspaceId(workspaceId);

      if (range) {
        const anchor =
          surfaceSelectionAnchor && ordered.includes(surfaceSelectionAnchor)
            ? surfaceSelectionAnchor
            : surfaceId;
        const from = ordered.indexOf(anchor);
        const to = ordered.indexOf(surfaceId);
        if (from >= 0 && to >= 0) {
          const [start, end] = from < to ? [from, to] : [to, from];
          setSelectedSurfaceIds(ordered.slice(start, end + 1));
        } else {
          setSelectedSurfaceIds([surfaceId]);
        }
        setSurfaceSelectionAnchor(anchor);
        return;
      }

      if (toggle) {
        if (current.includes(surfaceId)) {
          const next = current.filter((id) => id !== surfaceId);
          setSelectedSurfaceIds(next);
          setSurfaceSelectionAnchor(next.length > 0 ? surfaceId : null);
        } else {
          setSelectedSurfaceIds([...current, surfaceId]);
          setSurfaceSelectionAnchor(surfaceId);
        }
        return;
      }

      if (
        opts?.preserveIfAlreadySelected &&
        current.includes(surfaceId) &&
        current.length > 1
      ) {
        return;
      }

      setSelectedSurfaceIds([surfaceId]);
      setSurfaceSelectionAnchor(surfaceId);
    },
    [
      selectedSurfaceIds,
      selectionWorkspaceId,
      surfaceOrderByWorkspace,
      surfaceSelectionAnchor,
    ],
  );

  const resolvePaneContextSelection = useCallback(
    (workspaceId: string, paneId: string) => {
      if (
        selectionWorkspaceId === workspaceId &&
        selectedPaneIds.includes(paneId) &&
        selectedPaneIds.length > 0
      ) {
        return selectedPaneIds;
      }
      return [paneId];
    },
    [selectedPaneIds, selectionWorkspaceId],
  );

  const resolveSurfaceContextSelection = useCallback(
    (workspaceId: string, surfaceId: string) => {
      if (
        selectionWorkspaceId === workspaceId &&
        selectedSurfaceIds.includes(surfaceId) &&
        selectedSurfaceIds.length > 0
      ) {
        return selectedSurfaceIds;
      }
      return [surfaceId];
    },
    [selectedSurfaceIds, selectionWorkspaceId],
  );

  return {
    selectionWorkspaceId,
    setSelectionWorkspaceId,
    selectedPaneIds,
    setSelectedPaneIds,
    selectedSurfaceIds,
    setSelectedSurfaceIds,
    paneSelectionAnchor,
    setPaneSelectionAnchor,
    surfaceSelectionAnchor,
    setSurfaceSelectionAnchor,
    clearSelections,
    selectPaneInWorkspace,
    selectSurfaceInWorkspace,
    resolvePaneContextSelection,
    resolveSurfaceContextSelection,
  };
}
