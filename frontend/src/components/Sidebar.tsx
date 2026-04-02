import { useEffect, useMemo, useRef, useState } from "react";
import { allLeafIds, findLeaf } from "../lib/bspTree";
import { normalizeIconId } from "../lib/iconRegistry";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { SidebarActions } from "./sidebar/SidebarActions";
import { SidebarHeader } from "./sidebar/SidebarHeader";
import { SidebarMenus } from "./sidebar/SidebarMenus";
import { SidebarResizeHandle } from "./sidebar/SidebarResizeHandle";
import { SidebarTree } from "./sidebar/SidebarTree";
import type {
  ConfirmDialogState,
  IconPickerState,
  PaneMeta,
  SurfaceMeta,
  TreeContextMenu,
} from "./sidebar/sidebarTypes";
import { useSidebarOperations } from "./sidebar/useSidebarOperations";
import { useSidebarSelection } from "./sidebar/useSidebarSelection";

export function Sidebar() {
  const sidebarWidth = useWorkspaceStore((s) => s.sidebarWidth);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const setActiveWorkspace = useWorkspaceStore((s) => s.setActiveWorkspace);
  const createWorkspace = useWorkspaceStore((s) => s.createWorkspace);
  const closeWorkspace = useWorkspaceStore((s) => s.closeWorkspace);
  const renameWorkspace = useWorkspaceStore((s) => s.renameWorkspace);
  const setWorkspaceIcon = useWorkspaceStore((s) => s.setWorkspaceIcon);
  const createSurface = useWorkspaceStore((s) => s.createSurface);
  const createCanvasPanel = useWorkspaceStore((s) => s.createCanvasPanel);
  const setActiveSurface = useWorkspaceStore((s) => s.setActiveSurface);
  const closeSurface = useWorkspaceStore((s) => s.closeSurface);
  const closePane = useWorkspaceStore((s) => s.closePane);
  const setActivePaneId = useWorkspaceStore((s) => s.setActivePaneId);
  const setPaneName = useWorkspaceStore((s) => s.setPaneName);
  const setPaneIcon = useWorkspaceStore((s) => s.setPaneIcon);
  const splitActive = useWorkspaceStore((s) => s.splitActive);
  const focusCanvasPanel = useWorkspaceStore((s) => s.focusCanvasPanel);
  const setSidebarWidth = useWorkspaceStore((s) => s.setSidebarWidth);
  const toggleAgentPanel = useWorkspaceStore((s) => s.toggleAgentPanel);
  const toggleSystemMonitor = useWorkspaceStore((s) => s.toggleSystemMonitor);
  const toggleCommandPalette = useWorkspaceStore((s) => s.toggleCommandPalette);
  const toggleSearch = useWorkspaceStore((s) => s.toggleSearch);
  const toggleCommandHistory = useWorkspaceStore((s) => s.toggleCommandHistory);
  const toggleCommandLog = useWorkspaceStore((s) => s.toggleCommandLog);
  const toggleFileManager = useWorkspaceStore((s) => s.toggleFileManager);
  const toggleSessionVault = useWorkspaceStore((s) => s.toggleSessionVault);
  const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);
  const getUnread = useNotificationStore((s) => s.getUnreadForWorkspace);
  const notifications = useNotificationStore((s) => s.notifications);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const cognitiveEvents = useAgentMissionStore((s) => s.cognitiveEvents);
  const operationalEvents = useAgentMissionStore((s) => s.operationalEvents);

  const [query, setQuery] = useState("");
  const [collapsedWorkspaces, setCollapsedWorkspaces] = useState<Record<string, boolean>>({});
  const [collapsedSurfaces, setCollapsedSurfaces] = useState<Record<string, boolean>>({});
  const [editingWorkspaceId, setEditingWorkspaceId] = useState<string | null>(null);
  const [workspaceNameDraft, setWorkspaceNameDraft] = useState("");
  const [editingPaneId, setEditingPaneId] = useState<string | null>(null);
  const [paneNameDraft, setPaneNameDraft] = useState("");
  const [contextMenu, setContextMenu] = useState<TreeContextMenu | null>(null);
  const [iconPicker, setIconPicker] = useState<IconPickerState | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialogState | null>(null);

  const sidebarRef = useRef<HTMLDivElement>(null);
  const contextMenuRef = useRef<HTMLDivElement | null>(null);
  const iconPickerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let resizing = false;

    const handlePointerMove = (event: PointerEvent) => {
      if (!resizing) return;
      const sidebarLeft = sidebarRef.current?.getBoundingClientRect().left ?? 0;
      setSidebarWidth(event.clientX - sidebarLeft);
    };

    const handlePointerUp = () => {
      resizing = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);

    const startResize = () => {
      resizing = true;
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    };

    const handle = sidebarRef.current?.querySelector(
      "[data-sidebar-resize-handle='true']",
    );
    handle?.addEventListener("pointerdown", startResize);

    return () => {
      handle?.removeEventListener("pointerdown", startResize);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [setSidebarWidth]);

  useEffect(() => {
    if (!contextMenu) return;

    const closeMenu = (event: MouseEvent) => {
      if (contextMenuRef.current?.contains(event.target as Node)) {
        return;
      }
      setContextMenu(null);
    };

    window.addEventListener("mousedown", closeMenu);
    return () => window.removeEventListener("mousedown", closeMenu);
  }, [contextMenu]);

  useEffect(() => {
    if (!iconPicker) return;
    const close = (event: MouseEvent) => {
      if (iconPickerRef.current?.contains(event.target as Node)) {
        return;
      }
      setIconPicker(null);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [iconPicker]);

  const attentionByPane = useMemo(() => {
    const map = new Map<string, number>();

    for (const notification of notifications) {
      if (notification.isRead) continue;
      const paneId = notification.panelId ?? notification.paneId;
      if (!paneId) continue;
      map.set(paneId, (map.get(paneId) ?? 0) + 1);
    }

    for (const approval of approvals) {
      if (approval.status === "pending" && approval.handledAt === null) {
        map.set(approval.paneId, (map.get(approval.paneId) ?? 0) + 1);
      }
    }
    return map;
  }, [approvals, notifications]);

  const filteredWorkspaces = useMemo(() => {
    const lower = query.trim().toLowerCase();
    if (!lower) return workspaces;

    return workspaces.filter((workspace) => {
      if (
        workspace.name.toLowerCase().includes(lower) ||
        workspace.cwd.toLowerCase().includes(lower) ||
        (workspace.gitBranch ?? "").toLowerCase().includes(lower)
      ) {
        return true;
      }

      return workspace.surfaces.some((surface) => {
        if (
          surface.name.toLowerCase().includes(lower) ||
          surface.icon.toLowerCase().includes(lower)
        ) {
          return true;
        }

        return allLeafIds(surface.layout).some((paneId) => {
          const paneName = (surface.paneNames[paneId] ?? paneId).toLowerCase();
          const paneIcon = normalizeIconId(surface.paneIcons[paneId]).toLowerCase();
          return paneName.includes(lower) || paneIcon.includes(lower);
        });
      });
    });
  }, [query, workspaces]);

  const paneMetaById = useMemo(() => {
    const map = new Map<string, PaneMeta>();

    for (const workspace of workspaces) {
      for (const surface of workspace.surfaces) {
        for (const paneId of allLeafIds(surface.layout)) {
          const panel = surface.canvasPanels.find((entry) => entry.paneId === paneId);
          map.set(paneId, {
            workspaceId: workspace.id,
            surfaceId: surface.id,
            layoutMode: surface.layoutMode,
            paneName: surface.paneNames[paneId] ?? paneId,
            paneIcon: normalizeIconId(surface.paneIcons[paneId]),
            sessionId:
              panel?.sessionId ?? findLeaf(surface.layout, paneId)?.sessionId ?? null,
            panel: panel
              ? { x: panel.x, y: panel.y, width: panel.width, height: panel.height }
              : undefined,
          });
        }
      }
    }

    return map;
  }, [workspaces]);

  const surfaceMetaById = useMemo(() => {
    const map = new Map<string, SurfaceMeta>();
    for (const workspace of workspaces) {
      for (const surface of workspace.surfaces) {
        map.set(surface.id, { workspaceId: workspace.id, name: surface.name });
      }
    }
    return map;
  }, [workspaces]);

  const paneOrderByWorkspace = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const workspace of filteredWorkspaces) {
      if (collapsedWorkspaces[workspace.id] ?? false) {
        map.set(workspace.id, []);
        continue;
      }
      const order: string[] = [];
      for (const surface of workspace.surfaces) {
        if (collapsedSurfaces[surface.id] ?? false) continue;
        order.push(...allLeafIds(surface.layout));
      }
      map.set(workspace.id, order);
    }
    return map;
  }, [collapsedSurfaces, collapsedWorkspaces, filteredWorkspaces]);

  const surfaceOrderByWorkspace = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const workspace of filteredWorkspaces) {
      if (collapsedWorkspaces[workspace.id] ?? false) {
        map.set(workspace.id, []);
        continue;
      }
      map.set(
        workspace.id,
        workspace.surfaces.map((surface) => surface.id),
      );
    }
    return map;
  }, [collapsedWorkspaces, filteredWorkspaces]);

  const {
    selectionWorkspaceId,
    setSelectionWorkspaceId,
    selectedPaneIds,
    setSelectedPaneIds,
    selectedSurfaceIds,
    setSelectedSurfaceIds,
    setPaneSelectionAnchor,
    setSurfaceSelectionAnchor,
    clearSelections,
    selectPaneInWorkspace,
    selectSurfaceInWorkspace,
    resolvePaneContextSelection,
    resolveSurfaceContextSelection,
  } = useSidebarSelection(
    paneMetaById,
    surfaceMetaById,
    paneOrderByWorkspace,
    surfaceOrderByWorkspace,
  );

  const {
    handleWorkspaceContextAction,
    handleSurfaceContextAction,
    handlePaneContextAction,
  } = useSidebarOperations({
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
  });

  const paneContextSelection = useMemo(() => {
    if (!contextMenu || contextMenu.kind !== "pane") return [];
    return resolvePaneContextSelection(contextMenu.workspaceId, contextMenu.paneId);
  }, [contextMenu, resolvePaneContextSelection]);

  const surfaceContextSelection = useMemo(() => {
    if (!contextMenu || contextMenu.kind !== "surface") return [];
    return resolveSurfaceContextSelection(
      contextMenu.workspaceId,
      contextMenu.surfaceId,
    );
  }, [contextMenu, resolveSurfaceContextSelection]);

  return (
    <div
      ref={sidebarRef}
      style={{
        height: "100%",
        width: `${sidebarWidth}px`,
        minWidth: `${sidebarWidth}px`,
        maxWidth: `${sidebarWidth}px`,
        minHeight: 0,
        background: "var(--bg-primary)",
        borderRight: "1px solid var(--border)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        position: "relative",
      }}
    >
      <SidebarHeader
        workspacesCount={workspaces.length}
        approvalsCount={approvals.filter((entry) => entry.status === "pending").length}
        reasoningCount={cognitiveEvents.length}
        createWorkspace={(layoutMode) =>
          createWorkspace(undefined, layoutMode ? { layoutMode } : undefined)
        }
        query={query}
        setQuery={setQuery}
      />

      <div style={{ flex: 1, overflow: "auto", padding: "var(--space-2) var(--space-3)" }}>
        <SidebarTree
          filteredWorkspaces={filteredWorkspaces}
          activeWorkspaceId={activeWorkspaceId}
          collapsedWorkspaces={collapsedWorkspaces}
          setCollapsedWorkspaces={setCollapsedWorkspaces}
          collapsedSurfaces={collapsedSurfaces}
          setCollapsedSurfaces={setCollapsedSurfaces}
          contextMenu={contextMenu}
          setContextMenu={setContextMenu}
          clearSelections={clearSelections}
          setActiveWorkspace={setActiveWorkspace}
          editingWorkspaceId={editingWorkspaceId}
          workspaceNameDraft={workspaceNameDraft}
          setWorkspaceNameDraft={setWorkspaceNameDraft}
          renameWorkspace={renameWorkspace}
          setEditingWorkspaceId={setEditingWorkspaceId}
          getUnread={getUnread}
          selectionWorkspaceId={selectionWorkspaceId}
          selectedSurfaceIds={selectedSurfaceIds}
          selectSurfaceInWorkspace={selectSurfaceInWorkspace}
          setActiveSurface={setActiveSurface}
          selectedPaneIds={selectedPaneIds}
          attentionByPane={attentionByPane}
          editingPaneId={editingPaneId}
          setEditingPaneId={setEditingPaneId}
          paneNameDraft={paneNameDraft}
          setPaneNameDraft={setPaneNameDraft}
          setPaneName={setPaneName}
          selectPaneInWorkspace={selectPaneInWorkspace}
          setActivePaneId={(paneId) => {
            if (paneId) {
              setActivePaneId(paneId);
            }
          }}
          focusCanvasPanel={focusCanvasPanel}
        />
      </div>

      <SidebarActions
        workspacesCount={workspaces.length}
        toggleAgentPanel={toggleAgentPanel}
        toggleSystemMonitor={toggleSystemMonitor}
        toggleCommandPalette={toggleCommandPalette}
        toggleSearch={toggleSearch}
        toggleCommandHistory={toggleCommandHistory}
        toggleCommandLog={toggleCommandLog}
        toggleFileManager={toggleFileManager}
        toggleSessionVault={toggleSessionVault}
        toggleSettings={toggleSettings}
      />

      <SidebarResizeHandle />

      <SidebarMenus
        contextMenu={contextMenu}
        contextMenuRef={contextMenuRef}
        paneContextSelection={paneContextSelection}
        surfaceContextSelection={surfaceContextSelection}
        handleWorkspaceContextAction={handleWorkspaceContextAction}
        handleSurfaceContextAction={handleSurfaceContextAction}
        handlePaneContextAction={handlePaneContextAction}
        iconPicker={iconPicker}
        iconPickerRef={iconPickerRef}
        setWorkspaceIcon={setWorkspaceIcon}
        setPaneIcon={setPaneIcon}
        setIconPicker={setIconPicker}
        confirmDialog={confirmDialog}
        setConfirmDialog={setConfirmDialog}
      />
    </div>
  );
}
