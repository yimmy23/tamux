import { allLeafIds, findLeaf } from "../../../lib/bspTree";
import type { AgentQueueTask } from "../../../lib/agentTaskQueue";
import type { Workspace } from "../../../lib/types";
import type { WorkContextEntry } from "../../../lib/agentWorkContext";
import type { TaskWorkspaceLocation } from "./types";

export const heartbeatColors: Record<string, string> = {
  ok: "var(--success)",
  alert: "var(--warning)",
  error: "var(--danger)",
};

export function workContextKindLabel(entry: WorkContextEntry): string {
  const kind = entry.changeKind;
  switch (kind) {
    case "added":
      return "Added";
    case "deleted":
      return "Deleted";
    case "renamed":
      return "Renamed";
    case "copied":
      return "Copied";
    case "untracked":
      return "Untracked";
    case "conflict":
      return "Conflict";
    case "modified":
      return "Modified";
    default:
      if (entry.kind === "generated_skill") return "Skill";
      if (entry.kind === "artifact") return "Artifact";
      return "Changed";
  }
}

export function workContextKindColor(entry: WorkContextEntry): string {
  switch (entry.changeKind) {
    case "added":
    case "copied":
    case "untracked":
      return "var(--success)";
    case "deleted":
      return "var(--danger)";
    case "renamed":
      return "var(--accent)";
    case "conflict":
      return "var(--warning)";
    default:
      if (entry.kind === "generated_skill") return "var(--mission)";
      if (entry.kind === "artifact") return "var(--accent)";
      return "var(--text-secondary)";
  }
}

export function taskLooksLikeCoding(task: AgentQueueTask): boolean {
  const haystack = `${task.title} ${task.description} ${task.command ?? ""}`.toLowerCase();
  return /(code|coding|repo|git|diff|patch|file|files|test|build|compile|fix|bug|rust|typescript|frontend|backend|refactor|implement)/.test(haystack);
}

export function findTaskWorkspaceLocation(workspaces: Workspace[], sessionId: string | null | undefined): TaskWorkspaceLocation | null {
  if (!sessionId) {
    return null;
  }

  for (const workspace of workspaces) {
    for (const surface of workspace.surfaces) {
      for (const paneId of allLeafIds(surface.layout)) {
        const leafSessionId = findLeaf(surface.layout, paneId)?.sessionId ?? null;
        const panel = surface.canvasPanels.find((entry) => entry.paneId === paneId) ?? null;
        const paneSessionId = panel?.sessionId ?? leafSessionId;
        if (paneSessionId !== sessionId) {
          continue;
        }

        return {
          workspaceId: workspace.id,
          workspaceName: workspace.name,
          surfaceId: surface.id,
          surfaceName: surface.name,
          paneId,
          cwd: panel?.cwd ?? workspace.cwd ?? null,
        };
      }
    }
  }

  return null;
}
