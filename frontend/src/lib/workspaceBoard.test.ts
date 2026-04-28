import { describe, expect, it } from "vitest";
import {
  actorLabel,
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
});
