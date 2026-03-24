import { getBridge } from "./bridge";

export interface WorkContextEntry {
  path: string;
  previousPath?: string | null;
  kind?: "repo_change" | "artifact" | "generated_skill" | null;
  source: string;
  changeKind?: string | null;
  repoRoot?: string | null;
  goalRunId?: string | null;
  stepIndex?: number | null;
  sessionId?: string | null;
  isText: boolean;
  updatedAt: number;
}

export interface ThreadWorkContext {
  threadId: string;
  entries: WorkContextEntry[];
}

export interface FilePreviewPayload {
  path: string;
  content: string;
  truncated: boolean;
  isText: boolean;
}

function normalizeEntry(raw: unknown): WorkContextEntry | null {
  const entry = raw && typeof raw === "object" ? raw as Record<string, unknown> : null;
  const path = typeof entry?.path === "string" ? entry.path.trim() : "";
  if (!path) return null;

  return {
    path,
    previousPath: typeof entry?.previous_path === "string"
      ? entry.previous_path
      : typeof entry?.previousPath === "string"
        ? entry.previousPath
        : null,
    kind: entry?.kind === "repo_change" || entry?.kind === "artifact" || entry?.kind === "generated_skill"
      ? entry.kind
      : null,
    source: typeof entry?.source === "string" ? entry.source : "unknown",
    changeKind: typeof entry?.change_kind === "string"
      ? entry.change_kind
      : typeof entry?.changeKind === "string"
        ? entry.changeKind
        : null,
    repoRoot: typeof entry?.repo_root === "string"
      ? entry.repo_root
      : typeof entry?.repoRoot === "string"
        ? entry.repoRoot
        : null,
    goalRunId: typeof entry?.goal_run_id === "string"
      ? entry.goal_run_id
      : typeof entry?.goalRunId === "string"
        ? entry.goalRunId
        : null,
    stepIndex: typeof entry?.step_index === "number"
      ? entry.step_index
      : typeof entry?.stepIndex === "number"
        ? entry.stepIndex
        : null,
    sessionId: typeof entry?.session_id === "string"
      ? entry.session_id
      : typeof entry?.sessionId === "string"
        ? entry.sessionId
        : null,
    isText: entry?.is_text === false ? false : entry?.isText === false ? false : true,
    updatedAt: typeof entry?.updated_at === "number"
      ? entry.updated_at
      : typeof entry?.updatedAt === "number"
        ? entry.updatedAt
        : Date.now(),
  };
}

function normalizeThreadWorkContext(raw: unknown, fallbackThreadId?: string): ThreadWorkContext {
  const value = raw && typeof raw === "object" ? raw as Record<string, unknown> : {};
  const context = value.context && typeof value.context === "object"
    ? value.context as Record<string, unknown>
    : value;
  const threadId = typeof context.thread_id === "string"
    ? context.thread_id
    : typeof context.threadId === "string"
      ? context.threadId
      : typeof value.thread_id === "string"
        ? value.thread_id
        : fallbackThreadId ?? "";
  const entriesRaw = Array.isArray(context.entries) ? context.entries : [];

  return {
    threadId,
    entries: entriesRaw
      .map((entry) => normalizeEntry(entry))
      .filter((entry): entry is WorkContextEntry => Boolean(entry))
      .sort((a, b) => b.updatedAt - a.updatedAt),
  };
}

export async function fetchThreadWorkContext(threadId: string): Promise<ThreadWorkContext> {
  const bridge = getBridge();
  if (!bridge?.agentGetWorkContext || !threadId) {
    return { threadId, entries: [] };
  }

  try {
    const result = await bridge.agentGetWorkContext(threadId);
    return normalizeThreadWorkContext(result, threadId);
  } catch {
    return { threadId, entries: [] };
  }
}

export async function fetchGitDiff(repoPath: string, filePath?: string | null): Promise<string> {
  const bridge = getBridge();
  if (!bridge?.agentGetGitDiff || !repoPath) {
    return "";
  }

  try {
    const result = await bridge.agentGetGitDiff(repoPath, filePath ?? null);
    if (typeof result === "string") {
      return result;
    }
    if (result && typeof result === "object" && typeof (result as Record<string, unknown>).diff === "string") {
      return (result as Record<string, string>).diff;
    }
    return "";
  } catch {
    return "";
  }
}

export async function fetchFilePreview(path: string, maxBytes = 65536): Promise<FilePreviewPayload | null> {
  const bridge = getBridge();
  if (!bridge?.agentGetFilePreview || !path) {
    return null;
  }

  try {
    const result = await bridge.agentGetFilePreview(path, maxBytes);
    if (!result || typeof result !== "object") {
      return null;
    }
    const payload = result as Record<string, unknown>;
    return {
      path: typeof payload.path === "string" ? payload.path : path,
      content: typeof payload.content === "string" ? payload.content : "",
      truncated: payload.truncated === true,
      isText: payload.is_text === true || payload.isText === true,
    };
  } catch {
    return null;
  }
}
