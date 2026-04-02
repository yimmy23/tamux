import { allLeafIds, findLeaf } from "../bspTree";
import { useSnippetStore, resolveSnippetTemplate } from "../snippetStore";
import { getTerminalController } from "../terminalRegistry";
import { useWorkspaceStore } from "../workspaceStore";
import type { ToolResult } from "./types";
import { resolvePaneIdByRef, resolveSurfaceIdByRef, resolveWorkspaceIdByRef } from "./workspaceHelpers";

export function executeListTerminals(callId: string, name: string): ToolResult {
  const store = useWorkspaceStore.getState();
  const workspace = store.activeWorkspace();
  if (!workspace) {
    return { toolCallId: callId, name, content: "No active workspace." };
  }

  const activeSurface = store.activeSurface();
  const activePaneId = activeSurface?.activePaneId ?? null;
  const lines: string[] = [`Workspace "${workspace.name}" (active):`];

  for (const surface of workspace.surfaces) {
    const paneIds = allLeafIds(surface.layout);
    const isActiveSurface = surface.id === activeSurface?.id;
    const modeLabel = surface.layoutMode === "canvas" ? "canvas" : "bsp";
    lines.push(`  Surface "${surface.name}"${isActiveSurface ? " (active" : " ("}${isActiveSurface ? ", " : ""}${modeLabel}):`);
    const canvasPanelMap = new Map(surface.canvasPanels?.map((panel) => [panel.paneId, panel]) ?? []);
    for (const paneId of paneIds) {
      const isActive = paneId === activePaneId && isActiveSurface;
      const paneName = surface.paneNames[paneId] || paneId;
      const canvasPanel = canvasPanelMap.get(paneId);
      const panelType = canvasPanel?.panelType ?? "terminal";
      const parts = [`    - ${paneName} [${paneId}]`, `type=${panelType}`];
      if (panelType === "browser") {
        if (canvasPanel?.url) parts.push(`url=${canvasPanel.url}`);
      } else {
        const leaf = findLeaf(surface.layout, paneId);
        const sessionId = canvasPanel?.sessionId ?? leaf?.sessionId ?? null;
        if (sessionId) parts.push(`session=${sessionId}`);
      }
      if (isActive) parts.push("(active)");
      lines.push(parts.join(" "));
    }
  }

  if (lines.length <= 1) {
    return { toolCallId: callId, name, content: "No terminal panes found." };
  }
  return { toolCallId: callId, name, content: lines.join("\n") };
}

export function executeListWorkspaces(callId: string, name: string): ToolResult {
  const store = useWorkspaceStore.getState();
  if (store.workspaces.length === 0) {
    return { toolCallId: callId, name, content: "No workspaces found." };
  }
  const lines: string[] = [];
  for (const workspace of store.workspaces) {
    const workspaceActive = workspace.id === store.activeWorkspaceId;
    lines.push(`Workspace "${workspace.name}" [${workspace.id}]${workspaceActive ? " (active)" : ""}`);
    for (const surface of workspace.surfaces) {
      const surfaceActive = surface.id === workspace.activeSurfaceId;
      lines.push(`  Surface "${surface.name}" [${surface.id}]${surfaceActive ? " (active)" : ""}`);
      for (const paneId of allLeafIds(surface.layout)) {
        const paneName = surface.paneNames[paneId] || paneId;
        const paneActive = surfaceActive && paneId === surface.activePaneId;
        lines.push(`    Pane "${paneName}" [${paneId}]${paneActive ? " (active)" : ""}`);
      }
    }
  }
  return { toolCallId: callId, name, content: lines.join("\n") };
}

export function executeCreateWorkspace(callId: string, name: string, workspaceName?: string): ToolResult {
  const store = useWorkspaceStore.getState();
  store.createWorkspace(workspaceName?.trim() || undefined);
  const createdId = useWorkspaceStore.getState().activeWorkspaceId;
  const created = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === createdId);
  if (!created) {
    return { toolCallId: callId, name, content: "Workspace creation requested, but no active workspace detected." };
  }
  return { toolCallId: callId, name, content: `Created workspace "${created.name}" [${created.id}] and set it active.` };
}

export function executeSetActiveWorkspace(callId: string, name: string, workspaceRef?: string): ToolResult {
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  if (!workspaceId) {
    return { toolCallId: callId, name, content: `Error: Workspace not found for "${workspaceRef ?? ""}".` };
  }
  const store = useWorkspaceStore.getState();
  store.setActiveWorkspace(workspaceId);
  const workspace = useWorkspaceStore.getState().workspaces.find((entry) => entry.id === workspaceId);
  return { toolCallId: callId, name, content: `Active workspace set to "${workspace?.name ?? workspaceId}" [${workspaceId}].` };
}

export function executeCreateSurface(callId: string, name: string, workspaceRef?: string, surfaceName?: string): ToolResult {
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  if (!workspaceId) {
    return { toolCallId: callId, name, content: `Error: Workspace not found for "${workspaceRef ?? ""}".` };
  }

  const before = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId)?.surfaces.map((surface) => surface.id) ?? [];
  const store = useWorkspaceStore.getState();
  store.createSurface(workspaceId);
  const afterWorkspace = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId);
  if (!afterWorkspace) {
    return { toolCallId: callId, name, content: "Surface creation requested, but workspace disappeared." };
  }
  const createdSurface = afterWorkspace.surfaces.find((surface) => !before.includes(surface.id))
    ?? afterWorkspace.surfaces.find((surface) => surface.id === afterWorkspace.activeSurfaceId);
  if (!createdSurface) {
    return { toolCallId: callId, name, content: "Surface created, but could not resolve new surface ID." };
  }
  if (surfaceName?.trim()) {
    store.renameSurface(createdSurface.id, surfaceName.trim());
  }
  const resolved = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId)?.surfaces.find((surface) => surface.id === createdSurface.id);
  return { toolCallId: callId, name, content: `Created surface "${resolved?.name ?? createdSurface.id}" [${createdSurface.id}] in workspace [${workspaceId}].` };
}

export function executeSetActiveSurface(callId: string, name: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for "${surfaceRef ?? ""}".` };
  }
  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  const surface = useWorkspaceStore.getState().activeSurface();
  return { toolCallId: callId, name, content: `Active surface set to "${surface?.name ?? surfaceId}" [${surfaceId}].` };
}

export function executeSplitPane(callId: string, name: string, direction?: string, paneRef?: string, newPaneName?: string): ToolResult {
  if (direction !== "horizontal" && direction !== "vertical") {
    return { toolCallId: callId, name, content: "Error: direction must be 'horizontal' or 'vertical'." };
  }
  const store = useWorkspaceStore.getState();
  const targetPaneId = paneRef ? resolvePaneIdByRef(paneRef) : store.activePaneId();
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No pane available to split." };
  }
  store.setActivePaneId(targetPaneId);
  store.splitActive(direction, typeof newPaneName === "string" ? newPaneName : undefined);
  const activePaneId = useWorkspaceStore.getState().activePaneId();
  return { toolCallId: callId, name, content: `Split pane [${targetPaneId}] ${direction}. New active pane is [${activePaneId ?? "unknown"}].` };
}

export function executeRenamePane(callId: string, name: string, paneName?: string, paneRef?: string): ToolResult {
  const nextName = String(paneName ?? "").trim();
  if (!nextName) {
    return { toolCallId: callId, name, content: "Error: name is required." };
  }
  const store = useWorkspaceStore.getState();
  const targetPaneId = paneRef ? resolvePaneIdByRef(paneRef) : store.activePaneId();
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No pane found to rename." };
  }
  store.setPaneName(targetPaneId, nextName);
  return { toolCallId: callId, name, content: `Renamed pane [${targetPaneId}] to "${nextName}".` };
}

export function executeListSnippets(callId: string, name: string, owner?: string): ToolResult {
  const normalizedOwner = (owner ?? "both").toLowerCase();
  const allowed = normalizedOwner === "user" || normalizedOwner === "assistant" || normalizedOwner === "both" ? normalizedOwner : "both";
  const snippets = useSnippetStore.getState().snippets
    .filter((snippet) => allowed === "both" || snippet.owner === allowed)
    .sort((left, right) => {
      if (left.isFavorite !== right.isFavorite) return left.isFavorite ? -1 : 1;
      return right.updatedAt - left.updatedAt;
    });

  if (snippets.length === 0) {
    return { toolCallId: callId, name, content: `No snippets found for owner filter "${allowed}".` };
  }
  const lines = snippets.map((snippet) => {
    const preview = snippet.content.length > 80 ? `${snippet.content.slice(0, 80)}...` : snippet.content;
    return `- ${snippet.name} [${snippet.id}] owner=${snippet.owner} category=${snippet.category} :: ${preview}`;
  });
  return { toolCallId: callId, name, content: lines.join("\n") };
}

export function executeCreateSnippet(callId: string, name: string, args: Record<string, any>): ToolResult {
  const snippetName = String(args.name ?? "").trim();
  const snippetContent = String(args.content ?? "").trim();
  if (!snippetName || !snippetContent) {
    return { toolCallId: callId, name, content: "Error: name and content are required." };
  }
  const tags = Array.isArray(args.tags) ? args.tags.map((tag) => String(tag).trim()).filter(Boolean) : [];
  useSnippetStore.getState().addSnippet({
    name: snippetName,
    content: snippetContent,
    owner: "assistant",
    category: typeof args.category === "string" && args.category.trim() ? args.category.trim() : "General",
    description: typeof args.description === "string" ? args.description.trim() : "",
    tags,
  });
  return { toolCallId: callId, name, content: `Created assistant snippet "${snippetName}".` };
}

export async function executeRunSnippet(
  callId: string,
  name: string,
  snippetRef?: string,
  paneRef?: string,
  params?: Record<string, string>,
  execute?: boolean,
): Promise<ToolResult> {
  const ref = String(snippetRef ?? "").trim();
  if (!ref) {
    return { toolCallId: callId, name, content: "Error: snippet is required." };
  }
  const snippets = useSnippetStore.getState().snippets;
  const lower = ref.toLowerCase();
  const snippet = snippets.find((entry) => entry.id === ref) ?? snippets.find((entry) => entry.name.trim().toLowerCase() === lower);
  if (!snippet) {
    return { toolCallId: callId, name, content: `Error: Snippet not found for "${ref}".` };
  }
  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) {
    return { toolCallId: callId, name, content: "Error: No terminal pane found for snippet execution." };
  }
  const controller = getTerminalController(paneId);
  if (!controller) {
    return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not ready.` };
  }
  const templateParams: Record<string, string> = {};
  if (params && typeof params === "object") {
    for (const [key, value] of Object.entries(params)) {
      templateParams[key] = String(value ?? "");
    }
  }
  const resolved = resolveSnippetTemplate(snippet.content, templateParams);
  await controller.sendText(resolved, { execute: execute !== false, trackHistory: true });
  useSnippetStore.getState().incrementUseCount(snippet.id);
  return { toolCallId: callId, name, content: `Snippet "${snippet.name}" executed in pane [${paneId}]${execute === false ? " (without Enter)" : ""}.` };
}

export function executeSetLayoutPreset(callId: string, name: string, preset?: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const allowed = ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"] as const;
  if (!preset || !allowed.includes(preset as (typeof allowed)[number])) {
    return { toolCallId: callId, name, content: "Error: preset must be one of single, 2-columns, 3-columns, grid-2x2, main-stack." };
  }
  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for "${surfaceRef ?? ""}".` };
  }
  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  store.applyPresetLayout(preset as "single" | "2-columns" | "3-columns" | "grid-2x2" | "main-stack");
  return { toolCallId: callId, name, content: `Applied preset "${preset}" to surface [${surfaceId}].` };
}

export function executeEqualizeLayout(callId: string, name: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for "${surfaceRef ?? ""}".` };
  }
  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  store.equalizeLayout();
  return { toolCallId: callId, name, content: `Equalized layout ratios for surface [${surfaceId}].` };
}

export function executeOpenCanvasBrowser(callId: string, name: string, url?: string, panelName?: string): ToolResult {
  const store = useWorkspaceStore.getState();
  const surface = store.activeSurface();
  if (!surface || surface.layoutMode !== "canvas") {
    return { toolCallId: callId, name, content: "Error: No active canvas surface. Switch to a canvas surface first or create one." };
  }
  const rawUrl = url?.trim() || "https://google.com";
  const normalizedUrl = rawUrl.match(/^https?:\/\//) ? rawUrl : `https://${rawUrl}`;
  const paneId = store.createCanvasPanel(surface.id, {
    panelType: "browser",
    paneIcon: "web",
    paneName: panelName?.trim() || "Browser",
    url: normalizedUrl,
  });
  if (!paneId) {
    return { toolCallId: callId, name, content: "Error: Failed to create canvas browser panel." };
  }
  return {
    toolCallId: callId,
    name,
    content: `Created canvas browser panel [${paneId}]${url ? ` loading ${url}` : ""}. Use browser_navigate with pane="${paneId}" to navigate it.`,
  };
}
