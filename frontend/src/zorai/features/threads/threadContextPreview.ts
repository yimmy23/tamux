import type { WorkContextEntry } from "@/lib/agentWorkContext";

export type ThreadContextPreviewRequest =
  | {
    type: "git-diff";
    repoRoot: string;
    filePath: string;
  }
  | {
    type: "file-preview";
    path: string;
  };

export function threadContextEntryKey(entry: WorkContextEntry): string {
  return `${entry.source}:${entry.repoRoot ?? ""}:${entry.path}`;
}

export function threadContextEntryDisplayPath(entry: WorkContextEntry, shortenPath: (path: string) => string): string {
  return entry.repoRoot ? `${shortenPath(entry.repoRoot)}/${entry.path}` : shortenPath(entry.path);
}

export function previewRequestForWorkContextEntry(entry: WorkContextEntry): ThreadContextPreviewRequest {
  if (entry.repoRoot) {
    return {
      type: "git-diff",
      repoRoot: entry.repoRoot,
      filePath: entry.path,
    };
  }

  return {
    type: "file-preview",
    path: entry.path,
  };
}
