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

export function mergeWorkspaceSettings(settings: WorkspaceSettings[], tasks: WorkspaceTask[]): WorkspaceSettings[] {
  const byId = new Map(settings.map((workspace) => [workspace.workspace_id, workspace]));
  for (const task of tasks) {
    if (byId.has(task.workspace_id)) continue;
    byId.set(task.workspace_id, {
      workspace_id: task.workspace_id,
      workspace_root: null,
      operator: "user",
      created_at: task.created_at,
      updated_at: task.updated_at,
    });
  }
  return Array.from(byId.values()).sort((left, right) => left.workspace_id.localeCompare(right.workspace_id));
}

export function chooseWorkspaceWithTasks({
  currentWorkspaceId,
  defaultWorkspaceId,
  workspaces,
  tasksByWorkspace,
}: {
  currentWorkspaceId: string;
  defaultWorkspaceId: string;
  workspaces: WorkspaceSettings[];
  tasksByWorkspace: Record<string, WorkspaceTask[]>;
}): string {
  if ((tasksByWorkspace[currentWorkspaceId] ?? []).length > 0) return currentWorkspaceId;
  if (currentWorkspaceId !== defaultWorkspaceId) return currentWorkspaceId;
  const populated = workspaces.find((workspace) => (tasksByWorkspace[workspace.workspace_id] ?? []).length > 0);
  return populated?.workspace_id ?? currentWorkspaceId;
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
    reporter: actorValue(row.reporter ?? parseJsonValue(row.reporter_json)) ?? "user",
    assignee: actorValue(row.assignee ?? parseJsonValue(row.assignee_json)),
    reviewer: actorValue(row.reviewer ?? parseJsonValue(row.reviewer_json)),
    thread_id: nullableString(row.thread_id),
    goal_run_id: nullableString(row.goal_run_id),
    runtime_history: arrayValue(row.runtime_history ?? parseJsonValue(row.runtime_history_json)).map(runtimeHistoryValue),
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
      actor: actorValue(row.actor ?? parseJsonValue(row.actor_json)),
      created_at: numberValue(row.created_at),
    };
  }).filter((notice): notice is WorkspaceNotice => Boolean(notice));
}

export async function listWorkspaceSettings(): Promise<WorkspaceSettings[]> {
  const bridge = getBridge();
  try {
    const value = await bridge?.agentListWorkspaceSettings?.();
    const settings = Array.isArray(value) ? value.map(settingsValue).filter((entry): entry is WorkspaceSettings => Boolean(entry)) : [];
    const databaseTasks = await listWorkspaceTasksFromDatabaseRows(false);
    const databaseSettings = await listWorkspaceSettingsFromDatabaseRows(databaseTasks);
    return mergeWorkspaceSettings(settings.length > 0 ? settings : databaseSettings, databaseTasks);
  } catch {
    return await listWorkspaceSettingsFromDatabase();
  }
}

export async function getWorkspaceSettings(workspaceId: string): Promise<WorkspaceSettings | null> {
  try {
    const value = await getBridge()?.agentGetWorkspaceSettings?.(workspaceId);
    return settingsValue(value) ?? await getWorkspaceSettingsFromDatabase(workspaceId);
  } catch {
    return await getWorkspaceSettingsFromDatabase(workspaceId);
  }
}

export async function setWorkspaceOperator(workspaceId: string, operator: WorkspaceOperator): Promise<WorkspaceSettings | null> {
  const value = await getBridge()?.agentSetWorkspaceOperator?.(workspaceId, operator);
  return settingsValue(value);
}

export async function listWorkspaceTasks(workspaceId: string, includeDeleted = false): Promise<WorkspaceTask[]> {
  try {
    const value = await getBridge()?.agentListWorkspaceTasks?.(workspaceId, includeDeleted);
    const tasks = normalizeWorkspaceTasks(value);
    return tasks.length > 0 ? tasks : await listWorkspaceTasksFromDatabase(workspaceId, includeDeleted);
  } catch {
    return await listWorkspaceTasksFromDatabase(workspaceId, includeDeleted);
  }
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
  try {
    const notices = normalizeWorkspaceNotices(await getBridge()?.agentListWorkspaceNotices?.(workspaceId, taskId ?? null));
    return notices.length > 0 ? notices : await listWorkspaceNoticesFromDatabase(workspaceId, taskId);
  } catch {
    return await listWorkspaceNoticesFromDatabase(workspaceId, taskId);
  }
}

async function listWorkspaceSettingsFromDatabase(): Promise<WorkspaceSettings[]> {
  const tasks = await listWorkspaceTasksFromDatabaseRows(false);
  return listWorkspaceSettingsFromDatabaseRows(tasks);
}

async function listWorkspaceSettingsFromDatabaseRows(tasks: WorkspaceTask[]): Promise<WorkspaceSettings[]> {
  const rows = await queryDatabaseTableRows("workspace_settings", 500, "workspace_id", "asc");
  return mergeWorkspaceSettings(
    rows.map(settingsValue).filter((settings): settings is WorkspaceSettings => Boolean(settings)),
    tasks,
  );
}

async function getWorkspaceSettingsFromDatabase(workspaceId: string): Promise<WorkspaceSettings | null> {
  const settings = await listWorkspaceSettingsFromDatabase();
  return settings.find((entry) => entry.workspace_id === workspaceId) ?? null;
}

async function listWorkspaceTasksFromDatabase(workspaceId: string, includeDeleted: boolean): Promise<WorkspaceTask[]> {
  const tasks = await listWorkspaceTasksFromDatabaseRows(includeDeleted);
  return tasks
    .filter((task) => task.workspace_id === workspaceId)
    .filter((task) => includeDeleted || task.deleted_at == null);
}

async function listWorkspaceTasksFromDatabaseRows(includeDeleted: boolean): Promise<WorkspaceTask[]> {
  const rows = await queryDatabaseTableRows("workspace_tasks", 1000, "created_at", "asc");
  return normalizeWorkspaceTasks(rows)
    .filter((task) => includeDeleted || task.deleted_at == null);
}

async function listWorkspaceNoticesFromDatabase(workspaceId: string, taskId?: string | null): Promise<WorkspaceNotice[]> {
  const rows = await queryDatabaseTableRows("workspace_notices", 1000, "created_at", "asc");
  return normalizeWorkspaceNotices(rows)
    .filter((notice) => notice.workspace_id === workspaceId)
    .filter((notice) => !taskId || notice.task_id === taskId);
}

async function queryDatabaseTableRows(
  tableName: string,
  limit: number,
  sortColumn: string,
  sortDirection: "asc" | "desc",
): Promise<unknown[]> {
  const page = await getBridge()?.dbQueryDatabaseRows?.({
    tableName,
    offset: 0,
    limit,
    sortColumn,
    sortDirection,
  });
  const rows = page && typeof page === "object" && Array.isArray((page as { rows?: unknown }).rows)
    ? (page as { rows: unknown[] }).rows
    : [];
  return rows.map((row) => {
    if (row && typeof row === "object" && "values" in row) {
      const values = (row as { values?: unknown }).values;
      return values && typeof values === "object" ? values : {};
    }
    return row;
  });
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

function parseJsonValue(value: unknown): unknown {
  if (typeof value !== "string" || !value.trim()) return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
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
