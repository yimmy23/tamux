import { getBridge } from "./bridge";

export type WorkspaceOperator = "user" | "svarog";
export type WorkspaceTaskType = "thread" | "goal";
export type WorkspaceTaskStatus = "todo" | "in_progress" | "in_review" | "done";
export type WorkspacePriority = "low" | "normal" | "high" | "urgent";
export type WorkspaceActor = "user" | "svarog" | { agent: string } | { subagent: string } | null;

export type WorkspaceRuntimeHistoryEntry = {
  task_type: WorkspaceTaskType;
  thread_id: string | null;
  goal_run_id: string | null;
  agent_task_id: string | null;
  source: string | null;
  title: string | null;
  review_path: string | null;
  review_feedback: string | null;
  archived_at: number;
};

export type WorkspaceTask = {
  id: string;
  workspace_id: string;
  title: string;
  task_type: WorkspaceTaskType;
  description: string;
  definition_of_done: string | null;
  priority: WorkspacePriority;
  status: WorkspaceTaskStatus;
  sort_order: number;
  reporter: WorkspaceActor;
  assignee: WorkspaceActor;
  reviewer: WorkspaceActor;
  thread_id: string | null;
  goal_run_id: string | null;
  runtime_history: WorkspaceRuntimeHistoryEntry[];
  created_at: number;
  updated_at: number;
  started_at: number | null;
  completed_at: number | null;
  deleted_at: number | null;
  last_notice_id: string | null;
};

export type WorkspaceNotice = {
  id: string;
  workspace_id: string;
  task_id: string;
  notice_type: string;
  message: string;
  actor: WorkspaceActor;
  created_at: number;
};

export type WorkspaceSettings = {
  workspace_id: string;
  workspace_root: string | null;
  operator: WorkspaceOperator;
  created_at: number;
  updated_at: number;
};

export type WorkspaceColumn = {
  status: WorkspaceTaskStatus;
  title: string;
  tasks: WorkspaceTask[];
};

export type WorkspaceTaskCreate = {
  workspace_id: string;
  title: string;
  task_type: WorkspaceTaskType;
  description: string;
  definition_of_done?: string | null;
  priority?: WorkspacePriority | null;
  assignee?: WorkspaceActor;
  reviewer?: WorkspaceActor;
};

export type WorkspaceTaskUpdate = Partial<{
  title: string;
  description: string;
  definition_of_done: string | null;
  priority: WorkspacePriority;
  assignee: WorkspaceActor;
  reviewer: WorkspaceActor;
}>;

export const workspaceStatuses: Array<{ status: WorkspaceTaskStatus; title: string }> = [
  { status: "todo", title: "Todo" },
  { status: "in_progress", title: "In Progress" },
  { status: "in_review", title: "In Review" },
  { status: "done", title: "Done" },
];

export function projectWorkspaceColumns(tasks: WorkspaceTask[], includeDeleted = false): WorkspaceColumn[] {
  return workspaceStatuses.map(({ status, title }) => ({
    status,
    title,
    tasks: tasks
      .filter((task) => task.status === status)
      .filter((task) => includeDeleted || task.deleted_at == null)
      .sort((left, right) => left.sort_order - right.sort_order || left.created_at - right.created_at),
  }));
}

export function latestNoticeSummaries(notices: WorkspaceNotice[]): Record<string, string> {
  const latest = new Map<string, WorkspaceNotice>();
  for (const notice of notices) {
    const previous = latest.get(notice.task_id);
    if (!previous || notice.created_at >= previous.created_at) latest.set(notice.task_id, notice);
  }
  return Object.fromEntries(Array.from(latest, ([taskId, notice]) => [taskId, `${notice.notice_type}: ${notice.message}`]));
}

export function nextWorkspaceStatus(status: WorkspaceTaskStatus): WorkspaceTaskStatus {
  if (status === "todo") return "in_progress";
  if (status === "in_progress") return "in_review";
  if (status === "in_review") return "done";
  return "done";
}

export function taskRunBlocked(task: WorkspaceTask): boolean {
  return task.deleted_at == null && task.assignee == null;
}

export function actorLabel(actor: WorkspaceActor): string {
  if (!actor) return "none";
  if (actor === "user") return "user";
  if (actor === "svarog") return "svarog";
  if ("agent" in actor) return `agent:${actor.agent}`;
  return `subagent:${actor.subagent}`;
}

export function actorFromText(value: string): WorkspaceActor {
  const trimmed = value.trim();
  if (!trimmed || trimmed === "none") return null;
  if (trimmed === "user") return "user";
  if (trimmed === "svarog") return { agent: "svarog" };
  if (trimmed.startsWith("agent:")) return { agent: trimmed.slice("agent:".length).trim() };
  if (trimmed.startsWith("subagent:")) return { subagent: trimmed.slice("subagent:".length).trim() };
  return { agent: trimmed };
}

export function normalizeWorkspaceTasks(value: unknown): WorkspaceTask[] {
  if (!Array.isArray(value)) return [];
  return value.map(normalizeWorkspaceTask).filter((task): task is WorkspaceTask => Boolean(task));
}

export function normalizeWorkspaceTask(value: unknown): WorkspaceTask | null {
  if (!value || typeof value !== "object") return null;
  const row = value as Record<string, unknown>;
  const id = stringValue(row.id);
  if (!id) return null;
  return {
    id,
    workspace_id: stringValue(row.workspace_id) || "main",
    title: stringValue(row.title) || id,
    task_type: taskTypeValue(row.task_type),
    description: stringValue(row.description),
    definition_of_done: nullableString(row.definition_of_done),
    priority: priorityValue(row.priority),
    status: statusValue(row.status),
    sort_order: numberValue(row.sort_order),
    reporter: actorValue(row.reporter) ?? "user",
    assignee: actorValue(row.assignee),
    reviewer: actorValue(row.reviewer),
    thread_id: nullableString(row.thread_id),
    goal_run_id: nullableString(row.goal_run_id),
    runtime_history: Array.isArray(row.runtime_history) ? row.runtime_history.map(runtimeHistoryValue) : [],
    created_at: numberValue(row.created_at),
    updated_at: numberValue(row.updated_at),
    started_at: nullableNumber(row.started_at),
    completed_at: nullableNumber(row.completed_at),
    deleted_at: nullableNumber(row.deleted_at),
    last_notice_id: nullableString(row.last_notice_id),
  };
}

export function normalizeWorkspaceNotices(value: unknown): WorkspaceNotice[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const row = item && typeof item === "object" ? item as Record<string, unknown> : {};
    const id = stringValue(row.id);
    const taskId = stringValue(row.task_id);
    if (!id || !taskId) return null;
    return {
      id,
      workspace_id: stringValue(row.workspace_id) || "main",
      task_id: taskId,
      notice_type: stringValue(row.notice_type) || "notice",
      message: stringValue(row.message),
      actor: actorValue(row.actor),
      created_at: numberValue(row.created_at),
    };
  }).filter((notice): notice is WorkspaceNotice => Boolean(notice));
}

export async function listWorkspaceSettings(): Promise<WorkspaceSettings[]> {
  const bridge = getBridge();
  const value = await bridge?.agentListWorkspaceSettings?.();
  return Array.isArray(value) ? value.map(settingsValue).filter((settings): settings is WorkspaceSettings => Boolean(settings)) : [];
}

export async function getWorkspaceSettings(workspaceId: string): Promise<WorkspaceSettings | null> {
  const value = await getBridge()?.agentGetWorkspaceSettings?.(workspaceId);
  return settingsValue(value);
}

export async function setWorkspaceOperator(workspaceId: string, operator: WorkspaceOperator): Promise<WorkspaceSettings | null> {
  const value = await getBridge()?.agentSetWorkspaceOperator?.(workspaceId, operator);
  return settingsValue(value);
}

export async function listWorkspaceTasks(workspaceId: string, includeDeleted = false): Promise<WorkspaceTask[]> {
  const value = await getBridge()?.agentListWorkspaceTasks?.(workspaceId, includeDeleted);
  return normalizeWorkspaceTasks(value);
}

export async function createWorkspaceTask(request: WorkspaceTaskCreate): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentCreateWorkspaceTask?.(request));
}

export async function updateWorkspaceTask(taskId: string, update: WorkspaceTaskUpdate): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentUpdateWorkspaceTask?.(taskId, update));
}

export async function moveWorkspaceTask(taskId: string, status: WorkspaceTaskStatus, sortOrder?: number): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentMoveWorkspaceTask?.({ task_id: taskId, status, sort_order: sortOrder ?? null }));
}

export async function runWorkspaceTask(taskId: string): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentRunWorkspaceTask?.(taskId));
}

export async function pauseWorkspaceTask(taskId: string): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentPauseWorkspaceTask?.(taskId));
}

export async function stopWorkspaceTask(taskId: string): Promise<WorkspaceTask | null> {
  return normalizeWorkspaceTask(await getBridge()?.agentStopWorkspaceTask?.(taskId));
}

export async function deleteWorkspaceTask(taskId: string): Promise<boolean> {
  const value = await getBridge()?.agentDeleteWorkspaceTask?.(taskId);
  return Boolean(value && typeof value === "object" ? (value as { ok?: boolean }).ok !== false : value);
}

export async function listWorkspaceNotices(workspaceId: string, taskId?: string | null): Promise<WorkspaceNotice[]> {
  return normalizeWorkspaceNotices(await getBridge()?.agentListWorkspaceNotices?.(workspaceId, taskId ?? null));
}

function settingsValue(value: unknown): WorkspaceSettings | null {
  if (!value || typeof value !== "object") return null;
  const row = value as Record<string, unknown>;
  const workspaceId = stringValue(row.workspace_id);
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    workspace_root: nullableString(row.workspace_root),
    operator: row.operator === "svarog" ? "svarog" : "user",
    created_at: numberValue(row.created_at),
    updated_at: numberValue(row.updated_at),
  };
}

function runtimeHistoryValue(value: unknown): WorkspaceRuntimeHistoryEntry {
  const row = value && typeof value === "object" ? value as Record<string, unknown> : {};
  return {
    task_type: taskTypeValue(row.task_type),
    thread_id: nullableString(row.thread_id),
    goal_run_id: nullableString(row.goal_run_id),
    agent_task_id: nullableString(row.agent_task_id),
    source: nullableString(row.source),
    title: nullableString(row.title),
    review_path: nullableString(row.review_path),
    review_feedback: nullableString(row.review_feedback),
    archived_at: numberValue(row.archived_at),
  };
}

function actorValue(value: unknown): WorkspaceActor {
  if (value === "user") return "user";
  if (value === "svarog") return "svarog";
  if (!value || typeof value !== "object") return null;
  const row = value as Record<string, unknown>;
  const agent = stringValue(row.agent);
  if (agent) return { agent };
  const subagent = stringValue(row.subagent);
  if (subagent) return { subagent };
  return null;
}

function taskTypeValue(value: unknown): WorkspaceTaskType {
  return value === "goal" ? "goal" : "thread";
}

function statusValue(value: unknown): WorkspaceTaskStatus {
  return value === "in_progress" || value === "in_review" || value === "done" ? value : "todo";
}

function priorityValue(value: unknown): WorkspacePriority {
  return value === "low" || value === "high" || value === "urgent" ? value : "normal";
}

function nullableString(value: unknown): string | null {
  const next = stringValue(value);
  return next || null;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function numberValue(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function nullableNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}
