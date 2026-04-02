import type { CSSProperties } from "react";
import type { Surface, Workspace } from "../../lib/types";

export type TreeContextMenu =
  | {
      kind: "workspace";
      workspaceId: string;
      x: number;
      y: number;
    }
  | {
      kind: "surface";
      workspaceId: string;
      surfaceId: string;
      x: number;
      y: number;
    }
  | {
      kind: "pane";
      workspaceId: string;
      surfaceId: string;
      paneId: string;
      x: number;
      y: number;
    };

export type IconPickerState =
  | {
      kind: "workspace";
      workspaceId: string;
      x: number;
      y: number;
    }
  | {
      kind: "pane";
      paneIds: string[];
      x: number;
      y: number;
    };

export type ConfirmDialogState = {
  title: string;
  message: string;
  confirmLabel: string;
  tone: "danger" | "warning" | "neutral";
  action: () => void;
};

export type PaneMeta = {
  workspaceId: string;
  surfaceId: string;
  layoutMode: Surface["layoutMode"];
  paneName: string;
  paneIcon: string;
  sessionId: string | null;
  panel?: { x: number; y: number; width: number; height: number };
};

export type SurfaceMeta = {
  workspaceId: string;
  name: string;
};

export type SidebarTreeProps = {
  filteredWorkspaces: Workspace[];
  activeWorkspaceId: string | null;
  collapsedWorkspaces: Record<string, boolean>;
  setCollapsedWorkspaces: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  collapsedSurfaces: Record<string, boolean>;
  setCollapsedSurfaces: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  contextMenu: TreeContextMenu | null;
  setContextMenu: React.Dispatch<React.SetStateAction<TreeContextMenu | null>>;
  clearSelections: () => void;
  setActiveWorkspace: (workspaceId: string) => void;
  editingWorkspaceId: string | null;
  workspaceNameDraft: string;
  setWorkspaceNameDraft: React.Dispatch<React.SetStateAction<string>>;
  renameWorkspace: (workspaceId: string, name: string) => void;
  setEditingWorkspaceId: React.Dispatch<React.SetStateAction<string | null>>;
  getUnread: (workspaceId: string) => number;
  selectionWorkspaceId: string | null;
  selectedSurfaceIds: string[];
  selectSurfaceInWorkspace: (
    workspaceId: string,
    surfaceId: string,
    opts?: { toggle?: boolean; range?: boolean; preserveIfAlreadySelected?: boolean },
  ) => void;
  setActiveSurface: (surfaceId: string) => void;
  selectedPaneIds: string[];
  attentionByPane: Map<string, number>;
  editingPaneId: string | null;
  setEditingPaneId: React.Dispatch<React.SetStateAction<string | null>>;
  paneNameDraft: string;
  setPaneNameDraft: React.Dispatch<React.SetStateAction<string>>;
  setPaneName: (paneId: string, name: string) => void;
  selectPaneInWorkspace: (
    workspaceId: string,
    paneId: string,
    opts?: { toggle?: boolean; range?: boolean; preserveIfAlreadySelected?: boolean },
  ) => void;
  setActivePaneId: (paneId: string | null) => void;
  focusCanvasPanel: (
    paneId: string,
    options?: { storePreviousView?: boolean },
  ) => void;
};

export type SidebarMenusProps = {
  contextMenu: TreeContextMenu | null;
  contextMenuRef: React.RefObject<HTMLDivElement | null>;
  paneContextSelection: string[];
  surfaceContextSelection: string[];
  handleWorkspaceContextAction: (
    action: "rename" | "icon" | "append" | "new-canvas" | "close",
    workspaceId: string,
  ) => void;
  handleSurfaceContextAction: (
    action: "close",
    workspaceId: string,
    surfaceId: string,
  ) => void;
  handlePaneContextAction: (
    action: "rename" | "icon" | "append" | "duplicate" | "close",
    workspaceId: string,
    surfaceId: string,
    paneId: string,
  ) => void;
  iconPicker: IconPickerState | null;
  iconPickerRef: React.RefObject<HTMLDivElement | null>;
  setWorkspaceIcon: (workspaceId: string, icon: string) => void;
  setPaneIcon: (paneId: string, icon: string) => void;
  setIconPicker: React.Dispatch<React.SetStateAction<IconPickerState | null>>;
  confirmDialog: ConfirmDialogState | null;
  setConfirmDialog: React.Dispatch<React.SetStateAction<ConfirmDialogState | null>>;
};

export type SidebarStyleFn = (active: boolean) => CSSProperties;
