import type { LoadedCDUIView } from "../cduiLoader";
import type { ViewDocument } from "../../schemas/uiSchema";

export interface BuilderSelection {
  viewId: string;
  nodeId: string;
  componentType: string;
}

export interface BuilderInsertSpec {
  targetNodeId?: string;
  componentType?: string;
  blockId?: string;
}

export interface ViewBuilderState {
  isEditMode: boolean;
  activeViewId: string | null;
  selectedNode: BuilderSelection | null;
  openMenuNodeId: string | null;
  originalDocuments: Record<string, ViewDocument>;
  draftDocuments: Record<string, ViewDocument>;
  dirtyViewIds: Record<string, boolean>;
  syncLoadedViews: (views: LoadedCDUIView[]) => void;
  startEditing: (selection: BuilderSelection) => void;
  stopEditing: () => void;
  selectNode: (selection: BuilderSelection) => void;
  toggleNodeMenu: (nodeId: string | null) => void;
  closeNodeMenu: () => void;
  discardActiveView: () => void;
  replaceActiveViewDocument: (document: ViewDocument) => void;
  toggleSelectedNodeEditable: () => boolean;
  moveSelectedNode: (direction: "up" | "down") => boolean;
  insertChildIntoSelectedNode: (spec: BuilderInsertSpec) => boolean;
  duplicateSelectedNode: () => boolean;
  deleteSelectedNode: () => boolean;
  patchSelectedNodeProps: (patch: Record<string, unknown>) => boolean;
  patchSelectedNodeStyle: (patch: Record<string, unknown>) => boolean;
  promoteSelectedNodeToBlock: () => boolean;
  moveNodeToTarget: (draggedNodeId: string, targetNodeId: string) => boolean;
}
