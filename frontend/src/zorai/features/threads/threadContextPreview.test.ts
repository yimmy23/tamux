import { describe, expect, it } from "vitest";

import type { WorkContextEntry } from "@/lib/agentWorkContext";
import { previewRequestsForWorkContextEntry, threadContextEntryKey } from "./threadContextPreview";

function entry(overrides: Partial<WorkContextEntry>): WorkContextEntry {
  return {
    path: overrides.path ?? "src/App.tsx",
    kind: overrides.kind ?? "repo_change",
    source: overrides.source ?? "daemon",
    isText: overrides.isText ?? true,
    updatedAt: overrides.updatedAt ?? 1,
    ...overrides,
  };
}

describe("thread context preview helpers", () => {
  it("requests file preview plus optional git diff for repo-backed work context entries", () => {
    expect(previewRequestsForWorkContextEntry(entry({ repoRoot: "/repo", path: "src/App.tsx" }))).toEqual([
      {
        type: "file-preview",
        path: "/repo/src/App.tsx",
      },
      {
        type: "git-diff",
        repoRoot: "/repo",
        filePath: "src/App.tsx",
      },
    ]);
  });

  it("requests plain file preview for artifact entries without a repo root", () => {
    expect(previewRequestsForWorkContextEntry(entry({ repoRoot: null, path: "/tmp/result.txt" }))).toEqual([{
      type: "file-preview",
      path: "/tmp/result.txt",
    }]);
  });

  it("keeps entries with the same path but different repo roots distinct", () => {
    expect(threadContextEntryKey(entry({ source: "daemon", repoRoot: "/repo-a", path: "README.md" })))
      .not.toBe(threadContextEntryKey(entry({ source: "daemon", repoRoot: "/repo-b", path: "README.md" })));
  });
});
