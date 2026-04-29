import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchGoalRuns } from "./goalRuns";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("goalRuns", () => {
  it("falls back to the database goal_runs table when the agent list endpoint fails", async () => {
    const dbQueryDatabaseRows = vi.fn(async () => ({
      rows: [
        {
          values: {
            id: "goal-db-1",
            title: "Database goal",
            goal: "Recover rows from SQLite",
            status: "running",
            priority: "normal",
            created_at: 10,
            updated_at: 11,
            thread_id: "thread-main",
            execution_thread_ids_json: "[\"thread-main\",\"thread-worker\"]",
            memory_updates_json: "[\"captured\"]",
            child_task_ids_json: "[\"task-1\"]",
            model_usage_json: "[]",
            launch_assignment_snapshot_json: "[]",
            runtime_assignment_list_json: "[]",
          },
        },
      ],
    }));

    vi.stubGlobal("window", {
      zorai: {
        agentListGoalRuns: vi.fn(async () => {
          throw new Error("agent bridge unavailable");
        }),
        dbQueryDatabaseRows,
      },
    });

    const goals = await fetchGoalRuns();

    expect(dbQueryDatabaseRows).toHaveBeenCalledWith({
      tableName: "goal_runs",
      offset: 0,
      limit: 500,
      sortColumn: "created_at",
      sortDirection: "desc",
    });
    expect(goals).toHaveLength(1);
    expect(goals[0]).toMatchObject({
      id: "goal-db-1",
      title: "Database goal",
      goal: "Recover rows from SQLite",
      status: "running",
      thread_id: "thread-main",
      execution_thread_ids: ["thread-main", "thread-worker"],
      memory_updates: ["captured"],
      child_task_ids: ["task-1"],
    });
  });
});
