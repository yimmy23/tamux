import { create } from "zustand";
import type { ViewBuilderState } from "./types";
import {
  applyDraftMutation,
  cloneDocument,
  cloneNodeWithFreshIds,
  createInsertNode,
  deleteNodeFromTree,
  duplicateNodeInTree,
  findNodeInDocument,
  insertChildIntoTree,
  moveDocumentNode,
  moveNodeWithinDocument,
  resolveInsertionTargetId,
  toggleNodeEditable,
  transformDocumentTrees,
  updateDocumentNodeById,
} from "./tree";

export const useViewBuilderStore = create<ViewBuilderState>((set) => ({
  isEditMode: false,
  activeViewId: null,
  selectedNode: null,
  openMenuNodeId: null,
  originalDocuments: {},
  draftDocuments: {},
  dirtyViewIds: {},
  syncLoadedViews: (views) => set((state) => {
    const originalDocuments = { ...state.originalDocuments };
    const draftDocuments = { ...state.draftDocuments };

    for (const view of views) {
      originalDocuments[view.id] = cloneDocument(view.document);
      if (!state.dirtyViewIds[view.id]) {
        draftDocuments[view.id] = cloneDocument(view.document);
      }
    }

    return {
      originalDocuments,
      draftDocuments,
    };
  }),
  startEditing: (selection) => set({
    isEditMode: true,
    activeViewId: selection.viewId,
    selectedNode: selection,
    openMenuNodeId: null,
  }),
  stopEditing: () => set({
    isEditMode: false,
    activeViewId: null,
    selectedNode: null,
    openMenuNodeId: null,
  }),
  selectNode: (selection) => set((state) => ({
    isEditMode: state.isEditMode,
    activeViewId: selection.viewId,
    selectedNode: selection,
    openMenuNodeId: state.openMenuNodeId,
  })),
  toggleNodeMenu: (nodeId) => set((state) => ({
    openMenuNodeId: state.openMenuNodeId === nodeId ? null : nodeId,
  })),
  closeNodeMenu: () => set({ openMenuNodeId: null }),
  discardActiveView: () => set((state) => {
    if (!state.activeViewId) {
      return {};
    }

    const original = state.originalDocuments[state.activeViewId];
    if (!original) {
      return {};
    }

    return {
      draftDocuments: {
        ...state.draftDocuments,
        [state.activeViewId]: cloneDocument(original),
      },
      dirtyViewIds: {
        ...state.dirtyViewIds,
        [state.activeViewId]: false,
      },
    };
  }),
  replaceActiveViewDocument: (document) => set((state) => {
    if (!state.activeViewId) {
      return {};
    }

    return {
      originalDocuments: {
        ...state.originalDocuments,
        [state.activeViewId]: cloneDocument(document),
      },
      draftDocuments: {
        ...state.draftDocuments,
        [state.activeViewId]: cloneDocument(document),
      },
      dirtyViewIds: {
        ...state.dirtyViewIds,
        [state.activeViewId]: false,
      },
    };
  }),
  toggleSelectedNodeEditable: () => {
    let changed = false;
    set((state) => {
      if (!state.activeViewId || !state.selectedNode) {
        return {};
      }

      const draft = state.draftDocuments[state.activeViewId];
      if (!draft) {
        return {};
      }

      const result = toggleNodeEditable(draft, state.selectedNode);
      changed = result.changed;
      if (!result.changed) {
        return {};
      }

      return {
        draftDocuments: {
          ...state.draftDocuments,
          [state.activeViewId]: result.document,
        },
        dirtyViewIds: {
          ...state.dirtyViewIds,
          [state.activeViewId]: true,
        },
      };
    });

    return changed;
  },
  moveSelectedNode: (direction) => {
    let changed = false;
    set((state) => {
      if (!state.activeViewId || !state.selectedNode) {
        return {};
      }

      const draft = state.draftDocuments[state.activeViewId];
      if (!draft) {
        return {};
      }

      const result = moveDocumentNode(draft, state.selectedNode.nodeId, direction);
      changed = result.changed;
      if (!result.changed) {
        return {};
      }

      return {
        draftDocuments: {
          ...state.draftDocuments,
          [state.activeViewId]: result.document,
        },
        dirtyViewIds: {
          ...state.dirtyViewIds,
          [state.activeViewId]: true,
        },
      };
    });

    return changed;
  },
  insertChildIntoSelectedNode: (spec) => {
    let changed = false;
    set((state) => applyDraftMutation(state, (draft, selectedNodeId) => {
      const insertNode = createInsertNode(spec);
      if (!insertNode) {
        changed = false;
        return { document: draft, changed: false };
      }

      const targetNodeId = resolveInsertionTargetId(draft, spec.targetNodeId ?? selectedNodeId);
      const result = transformDocumentTrees(draft, (node) => insertChildIntoTree(node, targetNodeId, insertNode));
      changed = result.changed;
      return result;
    }));
    return changed;
  },
  duplicateSelectedNode: () => {
    let changed = false;
    set((state) => applyDraftMutation(state, (draft, selectedNodeId) => {
      const result = transformDocumentTrees(draft, (node) => duplicateNodeInTree(node, selectedNodeId));
      changed = result.changed;
      return result;
    }));
    return changed;
  },
  deleteSelectedNode: () => {
    let changed = false;
    set((state) => {
      const nextState = applyDraftMutation(state, (draft, selectedNodeId) => {
        const result = transformDocumentTrees(draft, (node) => deleteNodeFromTree(node, selectedNodeId));
        changed = result.changed;
        return result;
      });

      if (!changed) {
        return nextState;
      }

      return {
        ...nextState,
        selectedNode: null,
      };
    });
    return changed;
  },
  patchSelectedNodeProps: (patch) => {
    let changed = false;
    set((state) => applyDraftMutation(state, (draft, selectedNodeId) => {
      const result = updateDocumentNodeById(draft, selectedNodeId, (target) => ({
        ...target,
        props: {
          ...(target.props ?? {}),
          ...patch,
        },
      }));
      changed = result.changed;
      return result;
    }));
    return changed;
  },
  patchSelectedNodeStyle: (patch) => {
    let changed = false;
    set((state) => applyDraftMutation(state, (draft, selectedNodeId) => {
      const result = updateDocumentNodeById(draft, selectedNodeId, (target) => ({
        ...target,
        props: {
          ...(target.props ?? {}),
          style: {
            ...(((target.props ?? {}).style as Record<string, unknown> | undefined) ?? {}),
            ...patch,
          },
        },
      }));
      changed = result.changed;
      return result;
    }));
    return changed;
  },
  promoteSelectedNodeToBlock: () => {
    let changed = false;
    set((state) => {
      if (!state.activeViewId || !state.selectedNode) {
        return {};
      }

      const draft = state.draftDocuments[state.activeViewId];
      if (!draft) {
        return {};
      }

      const selectedNode = findNodeInDocument(draft, state.selectedNode.nodeId);
      if (!selectedNode || selectedNode.use) {
        return {};
      }

      const blockKeyBase = `${(selectedNode.type ?? "block").replace(/[^a-zA-Z0-9]+/g, "-").toLowerCase()}-block`;
      let blockKey = blockKeyBase;
      let counter = 1;
      while (draft.blocks?.[blockKey]) {
        counter += 1;
        blockKey = `${blockKeyBase}-${counter}`;
      }

      const clonedLayout = cloneNodeWithFreshIds(selectedNode);
      const replaced = updateDocumentNodeById(draft, state.selectedNode.nodeId, (target) => ({
        id: target.id,
        use: blockKey,
        props: target.props,
        builder: {
          ...(target.builder ?? {}),
          editable: true,
        },
      }));

      if (!replaced.changed) {
        return {};
      }

      changed = true;
      return {
        draftDocuments: {
          ...state.draftDocuments,
          [state.activeViewId]: {
            ...replaced.document,
            blocks: {
              ...(replaced.document.blocks ?? {}),
              [blockKey]: {
                title: `${selectedNode.type ?? "Composite"} Block`,
                layout: clonedLayout,
                builder: {
                  category: "composite",
                  editable: true,
                },
              },
            },
          },
        },
        dirtyViewIds: {
          ...state.dirtyViewIds,
          [state.activeViewId]: true,
        },
      };
    });

    return changed;
  },
  moveNodeToTarget: (draggedNodeId, targetNodeId) => {
    let changed = false;
    set((state) => {
      if (!state.activeViewId) {
        return {};
      }

      const draft = state.draftDocuments[state.activeViewId];
      if (!draft) {
        return {};
      }

      const resolvedTargetNodeId = resolveInsertionTargetId(draft, targetNodeId);
      const result = moveNodeWithinDocument(draft, draggedNodeId, resolvedTargetNodeId);
      changed = result.changed;
      if (!result.changed) {
        return {};
      }

      return {
        draftDocuments: {
          ...state.draftDocuments,
          [state.activeViewId]: result.document,
        },
        dirtyViewIds: {
          ...state.dirtyViewIds,
          [state.activeViewId]: true,
        },
      };
    });

    return changed;
  },
}));
