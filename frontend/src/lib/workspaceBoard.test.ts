import { afterEach, describe, expect, it, vi } from "vitest";
import {
  actorLabel,
  chooseWorkspaceWithTasks,
  mergeWorkspaceSettings,
  listWorkspaceTasks,
  nextWorkspaceStatus,
  projectWorkspaceColumns,
  taskRunBlocked,
  WorkspaceTaskStatus,
  type WorkspaceTask,
} from "./workspaceBoard";

function task(id: string, status: WorkspaceTaskStatus, sortOrder: number, createdAt: number): WorkspaceTask {
  return {
    id,
    workspace_id: "main",
    title: id,
    task_type: "thread",
    description: "",
    definition_of_done: null,
    priority: "normal",
    status,
    sort_order: sortOrder,
    reporter: "user",
    assignee: null,
    reviewer: null,
    thread_id: null,
    goal_run_id: null,
    runtime_history: [],
    created_at: createdAt,
    updated_at: createdAt,
    started_at: null,
    completed_at: null,
    deleted_at: null,
    last_notice_id: null,
  };
}

describe("workspaceBoard", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("projects TUI workspace columns in status and sort order", () => {
    const columns = projectWorkspaceColumns([
      task("second", "todo", 20, 1),
      task("done", "done", 1, 1),
      task("first", "todo", 10, 2),
      { ...task("deleted", "todo", 1, 1), deleted_at: 10 },
    ]);

    expect(columns.map((column) => column.title)).toEqual(["Todo", "In Progress", "In Review", "Done"]);
    expect(columns[0].tasks.map((item) => item.id)).toEqual(["first", "second"]);
    expect(columns[3].tasks.map((item) => item.id)).toEqual(["done"]);
  });

  it("keeps TUI task action semantics", () => {
    expect(nextWorkspaceStatus("todo")).toBe("in_progress");
    expect(nextWorkspaceStatus("in_progress")).toBe("in_review");
    expect(nextWorkspaceStatus("in_review")).toBe("done");
    expect(taskRunBlocked(task("blocked", "todo", 1, 1))).toBe(true);
    expect(taskRunBlocked({ ...task("assigned", "todo", 1, 1), assignee: "svarog" })).toBe(false);
    expect(actorLabel({ agent: "svarog" })).toBe("agent:svarog");
  });

  it("adds task-only workspace ids to the workspace list", () => {
    expect(mergeWorkspaceSettings([
      { workspace_id: "main", workspace_root: null, operator: "user", created_at: 1, updated_at: 1 },
    ], [
      task("main-task", "todo", 1, 1),
      { ...task("repo-task", "todo", 1, 1), workspace_id: "repo-a" },
    ]).map((workspace) => workspace.workspace_id)).toEqual(["main", "repo-a"]);
  });

  it("chooses a populated workspace when the current default board is empty", () => {
    expect(chooseWorkspaceWithTasks({
      currentWorkspaceId: "main",
      defaultWorkspaceId: "main",
      workspaces: [
        { workspace_id: "main", workspace_root: null, operator: "user", created_at: 1, updated_at: 1 },
        { workspace_id: "repo-a", workspace_root: null, operator: "user", created_at: 1, updated_at: 1 },
      ],
      tasksByWorkspace: {
        main: [],
        "repo-a": [{ ...task("repo-task", "todo", 1, 1), workspace_id: "repo-a" }],
      },
    })).toBe("repo-a");
  });

  it("falls back to database workspace_tasks rows when the agent list endpoint fails", async () => {
    const dbQueryDatabaseRows = vi.fn(async () => ({
      rows: [
        {
          values: {
            id: "workspace-task-1",
            workspace_id: "main",
            title: "Recover workspace task",
            task_type: "goal",
            description: "Task exists in SQLite",
            definition_of_done: "Visible in React",
            priority: "high",
            status: "in_progress",
            sort_order: 7,
            reporter_json: "\"user\"",
            assignee_json: "{\"agent\":\"svarog\"}",
            reviewer_json: null,
            thread_id: "thread-1",
            goal_run_id: "goal-1",
            runtime_history_json: "[{\"task_type\":\"goal\",\"goal_run_id\":\"goal-1\",\"source\":\"workspace_runtime\",\"archived_at\":20}]",
            created_at: 10,
            updated_at: 11,
            started_at: 12,
            completed_at: null,
            deleted_at: null,
            last_notice_id: "notice-1",
          },
        },
      ],
    }));

    vi.stubGlobal("window", {
      zorai: {
        agentListWorkspaceTasks: vi.fn(async () => {
          throw new Error("agent bridge unavailable");
        }),
        dbQueryDatabaseRows,
      },
    });

    const tasks = await listWorkspaceTasks("main");

    expect(dbQueryDatabaseRows).toHaveBeenCalledWith({
      tableName: "workspace_tasks",
      offset: 0,
      limit: 1000,
      sortColumn: "created_at",
      sortDirection: "asc",
    });
    expect(tasks).toHaveLength(1);
    expect(tasks[0]).toMatchObject({
      id: "workspace-task-1",
      workspace_id: "main",
      task_type: "goal",
      status: "in_progress",
      assignee: { agent: "svarog" },
      goal_run_id: "goal-1",
      runtime_history: [
        {
          task_type: "goal",
          goal_run_id: "goal-1",
          source: "workspace_runtime",
          archived_at: 20,
        },
      ],
    });
  });
});
