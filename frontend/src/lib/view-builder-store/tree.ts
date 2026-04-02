import type { UINodeBuilderMeta, ViewDocument, UIViewNode } from "../../schemas/uiSchema";
import type { BuilderInsertSpec, BuilderSelection, ViewBuilderState } from "./types";

export const cloneDocument = (document: ViewDocument): ViewDocument => JSON.parse(JSON.stringify(document)) as ViewDocument;
const nextNodeId = (): string => `node_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

export const cloneNodeWithFreshIds = (node: UIViewNode): UIViewNode => ({
  ...node,
  id: nextNodeId(),
  ...(node.children ? { children: node.children.map(cloneNodeWithFreshIds) } : {}),
});

const findNodeById = (node: UIViewNode, nodeId: string): UIViewNode | null => {
  if (node.id === nodeId) {
    return node;
  }

  for (const child of node.children ?? []) {
    const match = findNodeById(child, nodeId);
    if (match) {
      return match;
    }
  }

  return null;
};

const nodeContainsId = (node: UIViewNode, nodeId: string): boolean => {
  if (node.id === nodeId) {
    return true;
  }

  return (node.children ?? []).some((child) => nodeContainsId(child, nodeId));
};

export const findNodeInDocument = (document: ViewDocument, nodeId: string): UIViewNode | null => (
  findNodeById(document.layout, nodeId)
  ?? (document.fallback ? findNodeById(document.fallback, nodeId) : null)
  ?? Object.values(document.blocks ?? {}).map((block) => findNodeById(block.layout, nodeId)).find(Boolean)
  ?? null
);

export const resolveInsertionTargetId = (document: ViewDocument, targetNodeId: string): string => {
  const targetNode = findNodeInDocument(document, targetNodeId);
  if (!targetNode?.use) {
    return targetNodeId;
  }

  const blockRootId = document.blocks?.[targetNode.use]?.layout.id;
  return blockRootId ?? targetNodeId;
};

export const updateNodeById = (
  node: UIViewNode,
  nodeId: string,
  updater: (target: UIViewNode) => UIViewNode,
): { nextNode: UIViewNode; changed: boolean } => {
  let changed = false;
  const nextNode = node.id === nodeId ? updater(node) : node;
  if (nextNode !== node) {
    changed = true;
  }

  const nextChildren = nextNode.children?.map((child) => {
    const result = updateNodeById(child, nodeId, updater);
    changed = changed || result.changed;
    return result.nextNode;
  });

  if (nextChildren && (changed || nextChildren.some((child, index) => child !== nextNode.children?.[index]))) {
    return {
      nextNode: {
        ...nextNode,
        children: nextChildren,
      },
      changed: true,
    };
  }

  return { nextNode, changed };
};

const reorderChildren = (
  children: UIViewNode[] | undefined,
  nodeId: string,
  direction: "up" | "down",
): { children: UIViewNode[] | undefined; changed: boolean } => {
  if (!children || children.length < 2) {
    return { children, changed: false };
  }

  const index = children.findIndex((child) => child.id === nodeId);
  if (index === -1) {
    return { children, changed: false };
  }

  const targetIndex = direction === "up" ? index - 1 : index + 1;
  if (targetIndex < 0 || targetIndex >= children.length) {
    return { children, changed: false };
  }

  const nextChildren = [...children];
  const [moved] = nextChildren.splice(index, 1);
  nextChildren.splice(targetIndex, 0, moved);
  return { children: nextChildren, changed: true };
};

const moveNodeInTree = (
  node: UIViewNode,
  nodeId: string,
  direction: "up" | "down",
): { nextNode: UIViewNode; changed: boolean } => {
  const reordered = reorderChildren(node.children, nodeId, direction);
  if (reordered.changed) {
    return {
      nextNode: {
        ...node,
        children: reordered.children,
      },
      changed: true,
    };
  }

  let changed = false;
  const nextChildren = node.children?.map((child) => {
    const result = moveNodeInTree(child, nodeId, direction);
    changed = changed || result.changed;
    return result.nextNode;
  });

  if (!changed) {
    return { nextNode: node, changed: false };
  }

  return {
    nextNode: {
      ...node,
      ...(nextChildren ? { children: nextChildren } : {}),
    },
    changed: true,
  };
};

export const updateDocumentNodeById = (
  document: ViewDocument,
  nodeId: string,
  updater: (target: UIViewNode) => UIViewNode,
): { document: ViewDocument; changed: boolean } => {
  let changed = false;
  const nextLayout = updateNodeById(document.layout, nodeId, updater);
  changed = changed || nextLayout.changed;

  const nextFallback = document.fallback ? updateNodeById(document.fallback, nodeId, updater) : null;
  changed = changed || Boolean(nextFallback?.changed);

  const nextBlocks = document.blocks
    ? Object.fromEntries(Object.entries(document.blocks).map(([key, block]) => {
      const result = updateNodeById(block.layout, nodeId, updater);
      changed = changed || result.changed;
      return [key, result.changed ? { ...block, layout: result.nextNode } : block];
    }))
    : undefined;

  if (!changed) {
    return { document, changed: false };
  }

  return {
    document: {
      ...document,
      layout: nextLayout.nextNode,
      ...(nextFallback ? { fallback: nextFallback.nextNode } : {}),
      ...(nextBlocks ? { blocks: nextBlocks } : {}),
    },
    changed: true,
  };
};

export const insertChildIntoTree = (
  node: UIViewNode,
  nodeId: string,
  newChild: UIViewNode,
): { nextNode: UIViewNode; changed: boolean } => {
  if (node.id === nodeId) {
    return {
      nextNode: {
        ...node,
        children: [...(node.children ?? []), newChild],
      },
      changed: true,
    };
  }

  let changed = false;
  const nextChildren = node.children?.map((child) => {
    const result = insertChildIntoTree(child, nodeId, newChild);
    changed = changed || result.changed;
    return result.nextNode;
  });

  if (!changed) {
    return { nextNode: node, changed: false };
  }

  return {
    nextNode: {
      ...node,
      ...(nextChildren ? { children: nextChildren } : {}),
    },
    changed: true,
  };
};

export const duplicateNodeInTree = (
  node: UIViewNode,
  nodeId: string,
): { nextNode: UIViewNode; changed: boolean } => {
  const index = node.children?.findIndex((child) => child.id === nodeId) ?? -1;
  if (index >= 0 && node.children) {
    const nextChildren = [...node.children];
    const sourceNode = node.children[index];
    if (!sourceNode) {
      return { nextNode: node, changed: false };
    }
    nextChildren.splice(index + 1, 0, cloneNodeWithFreshIds(sourceNode));
    return {
      nextNode: {
        ...node,
        children: nextChildren,
      },
      changed: true,
    };
  }

  let changed = false;
  const nextChildren = node.children?.map((child) => {
    const result = duplicateNodeInTree(child, nodeId);
    changed = changed || result.changed;
    return result.nextNode;
  });

  if (!changed) {
    return { nextNode: node, changed: false };
  }

  return {
    nextNode: {
      ...node,
      ...(nextChildren ? { children: nextChildren } : {}),
    },
    changed: true,
  };
};

export const deleteNodeFromTree = (
  node: UIViewNode,
  nodeId: string,
): { nextNode: UIViewNode; changed: boolean } => {
  if (!node.children?.length) {
    return { nextNode: node, changed: false };
  }

  const filteredChildren = node.children.filter((child) => child.id !== nodeId);
  if (filteredChildren.length !== node.children.length) {
    return {
      nextNode: {
        ...node,
        children: filteredChildren,
      },
      changed: true,
    };
  }

  let changed = false;
  const nextChildren = node.children.map((child) => {
    const result = deleteNodeFromTree(child, nodeId);
    changed = changed || result.changed;
    return result.nextNode;
  });

  if (!changed) {
    return { nextNode: node, changed: false };
  }

  return {
    nextNode: {
      ...node,
      children: nextChildren,
    },
    changed: true,
  };
};

export const removeNodeFromTree = (
  node: UIViewNode,
  nodeId: string,
): { nextNode: UIViewNode; changed: boolean; removedNode: UIViewNode | null } => {
  if (!node.children?.length) {
    return { nextNode: node, changed: false, removedNode: null };
  }

  const directMatch = node.children.find((child) => child.id === nodeId) ?? null;
  if (directMatch) {
    return {
      nextNode: {
        ...node,
        children: node.children.filter((child) => child.id !== nodeId),
      },
      changed: true,
      removedNode: directMatch,
    };
  }

  let changed = false;
  let removedNode: UIViewNode | null = null;
  const nextChildren = node.children.map((child) => {
    const result = removeNodeFromTree(child, nodeId);
    changed = changed || result.changed;
    removedNode = removedNode ?? result.removedNode;
    return result.nextNode;
  });

  if (!changed) {
    return { nextNode: node, changed: false, removedNode: null };
  }

  return {
    nextNode: {
      ...node,
      children: nextChildren,
    },
    changed: true,
    removedNode,
  };
};

export const moveDocumentNode = (
  document: ViewDocument,
  nodeId: string,
  direction: "up" | "down",
): { document: ViewDocument; changed: boolean } => {
  let changed = false;
  const nextLayout = moveNodeInTree(document.layout, nodeId, direction);
  changed = changed || nextLayout.changed;

  const nextFallback = document.fallback ? moveNodeInTree(document.fallback, nodeId, direction) : null;
  changed = changed || Boolean(nextFallback?.changed);

  const nextBlocks = document.blocks
    ? Object.fromEntries(Object.entries(document.blocks).map(([key, block]) => {
      const result = moveNodeInTree(block.layout, nodeId, direction);
      changed = changed || result.changed;
      return [key, result.changed ? { ...block, layout: result.nextNode } : block];
    }))
    : undefined;

  if (!changed) {
    return { document, changed: false };
  }

  return {
    document: {
      ...document,
      layout: nextLayout.nextNode,
      ...(nextFallback ? { fallback: nextFallback.nextNode } : {}),
      ...(nextBlocks ? { blocks: nextBlocks } : {}),
    },
    changed: true,
  };
};

export const transformDocumentTrees = (
  document: ViewDocument,
  transform: (node: UIViewNode) => { nextNode: UIViewNode; changed: boolean },
): { document: ViewDocument; changed: boolean } => {
  let changed = false;
  const nextLayout = transform(document.layout);
  changed = changed || nextLayout.changed;

  const nextFallback = document.fallback ? transform(document.fallback) : null;
  changed = changed || Boolean(nextFallback?.changed);

  const nextBlocks = document.blocks
    ? Object.fromEntries(Object.entries(document.blocks).map(([key, block]) => {
      const result = transform(block.layout);
      changed = changed || result.changed;
      return [key, result.changed ? { ...block, layout: result.nextNode } : block];
    }))
    : undefined;

  if (!changed) {
    return { document, changed: false };
  }

  return {
    document: {
      ...document,
      layout: nextLayout.nextNode,
      ...(nextFallback ? { fallback: nextFallback.nextNode } : {}),
      ...(nextBlocks ? { blocks: nextBlocks } : {}),
    },
    changed: true,
  };
};

export const moveNodeWithinDocument = (
  document: ViewDocument,
  draggedNodeId: string,
  targetNodeId: string,
): { document: ViewDocument; changed: boolean } => {
  const draggedNode = findNodeInDocument(document, draggedNodeId);
  if (!draggedNode || draggedNodeId === targetNodeId || nodeContainsId(draggedNode, targetNodeId)) {
    return { document, changed: false };
  }

  const removed = transformDocumentTrees(document, (node) => removeNodeFromTree(node, draggedNodeId));
  if (!removed.changed) {
    return { document, changed: false };
  }

  const movedNode = cloneDocument({ layout: draggedNode } as ViewDocument).layout;
  const inserted = transformDocumentTrees(removed.document, (node) => insertChildIntoTree(node, targetNodeId, movedNode));
  return inserted.changed ? inserted : { document, changed: false };
};

const createComponentNode = (componentType: string): UIViewNode => ({
  id: nextNodeId(),
  type: componentType,
  props: {
    visible: true,
  },
  builder: {
    editable: true,
  },
});

export const createInsertNode = (spec: BuilderInsertSpec): UIViewNode | null => {
  if (spec.blockId) {
    return {
      id: nextNodeId(),
      use: spec.blockId,
      builder: {
        editable: true,
        droppable: true,
      },
    };
  }

  if (spec.componentType) {
    return createComponentNode(spec.componentType);
  }

  return null;
};

export const applyDraftMutation = (
  state: ViewBuilderState,
  mutate: (draft: ViewDocument, selectedNodeId: string) => { document: ViewDocument; changed: boolean },
): Partial<ViewBuilderState> => {
  if (!state.activeViewId || !state.selectedNode) {
    return {};
  }

  const draft = state.draftDocuments[state.activeViewId];
  if (!draft) {
    return {};
  }

  const result = mutate(draft, state.selectedNode.nodeId);
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
};

export const toggleNodeEditable = (
  draft: ViewDocument,
  selection: BuilderSelection,
): { document: ViewDocument; changed: boolean } => (
  updateDocumentNodeById(draft, selection.nodeId, (target) => ({
    ...target,
    builder: {
      ...(target.builder ?? {}),
      editable: !(target.builder?.editable ?? false),
    } satisfies UINodeBuilderMeta,
  }))
);
