import { describe, expect, it } from "vitest";
import { deriveSpawnedAgentTree } from "./spawnedAgentTree.ts";

describe("deriveSpawnedAgentTree", () => {
  it("keeps the top-level ancestor as anchor when a descendant shares the same thread_id", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "ancestor",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "descendant",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-root",
          parent_task_id: "ancestor",
          parent_thread_id: "thread-root",
        },
      ],
      "thread-root",
    );

    expect(tree?.anchor?.item.id).toBe("ancestor");
    expect(tree?.roots).toHaveLength(0);
    expect(tree?.anchor?.children[0]?.item.id).toBe("descendant");
  });

  it("nests descendants by parent_task_id and keeps threadless nodes visible but closed", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "root-task",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "child-task",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-child",
          parent_task_id: "root-task",
          parent_thread_id: "thread-root",
        },
        {
          id: "leaf-task",
          status: "completed",
          created_at: 30,
          parent_task_id: "child-task",
          parent_thread_id: "thread-child",
        },
      ],
      "thread-root",
    );

    expect(tree.anchor?.item.id).toBe("root-task");
    expect(tree.roots).toHaveLength(0);
    expect(tree.anchor?.children[0]?.item.id).toBe("child-task");
    expect(tree.anchor?.children[0]?.openable).toBe(true);
    expect(tree.anchor?.children[0]?.live).toBe(true);
    expect(tree.anchor?.children[0]?.children[0]?.item.id).toBe("leaf-task");
    expect(tree.anchor?.children[0]?.children[0]?.openable).toBe(false);
    expect(tree.anchor?.children[0]?.children[0]?.live).toBe(false);
  });

  it("keeps multiple top-level items in the active thread context visible", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "root-task",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "child-a",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-a",
          parent_thread_id: "thread-root",
        },
        {
          id: "child-b",
          status: "completed",
          created_at: 30,
          thread_id: "thread-b",
          parent_thread_id: "thread-root",
        },
      ],
      "thread-root",
    );

    expect(tree.anchor?.item.id).toBe("root-task");
    expect(tree.roots.map((node) => node.item.id)).toEqual(["child-b", "child-a"]);
  });

  it("anchors spawned child threads even when their parent task record exists", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "root-task",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "child-task",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-child",
          parent_task_id: "root-task",
          parent_thread_id: "thread-root",
        },
        {
          id: "grandchild-task",
          status: "completed",
          created_at: 30,
          thread_id: "thread-grandchild",
          parent_task_id: "child-task",
          parent_thread_id: "thread-child",
        },
      ],
      "thread-child",
    );

    expect(tree?.anchor?.item.id).toBe("child-task");
    expect(tree?.roots).toHaveLength(0);
    expect(tree?.anchor?.children[0]?.item.id).toBe("grandchild-task");
  });

  it("resolves the visible root container from parent_thread_id when the active thread is the parent thread", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "unrelated",
          status: "completed",
          created_at: 5,
          thread_id: "thread-unrelated",
        },
        {
          id: "spawned-root",
          status: "in_progress",
          created_at: 15,
          thread_id: "thread-spawned",
          parent_thread_id: "thread-parent",
        },
      ],
      "thread-parent",
    );

    expect(tree.anchor).toBeNull();
    expect(tree.roots.map((node) => node.item.id)).toEqual(["spawned-root"]);
    expect(tree.roots[0]?.openable).toBe(true);
    expect(tree.roots[0]?.live).toBe(true);
  });

  it("keeps descendant nodes visible when an intermediate parent task is missing", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "root-task",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "orphan-child",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-orphan",
          parent_task_id: "missing-parent",
          parent_thread_id: "thread-root",
        },
        {
          id: "grandchild",
          status: "completed",
          created_at: 30,
          thread_id: "thread-grandchild",
          parent_task_id: "orphan-child",
          parent_thread_id: "thread-orphan",
        },
      ],
      "thread-root",
    );

    expect(tree.anchor?.item.id).toBe("root-task");
    expect(tree.roots.map((node) => node.item.id)).toEqual(["orphan-child"]);
    expect(tree.roots[0]?.children[0]?.item.id).toBe("grandchild");
    expect(tree.roots[0]?.children[0]?.openable).toBe(true);
  });

  it("keeps multiple sibling spawned children visible under one parent thread", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "sibling-a",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-a",
          parent_thread_id: "thread-parent",
        },
        {
          id: "sibling-b",
          status: "completed",
          created_at: 30,
          thread_id: "thread-b",
          parent_thread_id: "thread-parent",
        },
        {
          id: "nested-child",
          status: "in_progress",
          created_at: 40,
          thread_id: "thread-child",
          parent_task_id: "sibling-a",
          parent_thread_id: "thread-a",
        },
      ],
      "thread-parent",
    );

    expect(tree.anchor).toBeNull();
    expect(tree.roots.map((node) => node.item.id)).toEqual([
      "sibling-b",
      "sibling-a",
    ]);
    expect(tree.roots[1]?.children[0]?.item.id).toBe("nested-child");
  });

  it("nests real run-style items by task_id even when ids differ", () => {
    const tree = deriveSpawnedAgentTree(
      [
        {
          id: "run-root",
          task_id: "task-root",
          status: "in_progress",
          created_at: 10,
          thread_id: "thread-root",
        },
        {
          id: "run-child",
          task_id: "task-child",
          status: "in_progress",
          created_at: 20,
          thread_id: "thread-child",
          parent_task_id: "task-root",
          parent_thread_id: "thread-root",
        },
      ],
      "thread-root",
    );

    expect(tree.anchor?.item.task_id).toBe("task-root");
    expect(tree.anchor?.children[0]?.item.task_id).toBe("task-child");
    expect(tree.roots).toHaveLength(0);
  });

  it("canonicalizes duplicate task identities by newest record regardless of input order", () => {
    const older = {
      id: "root-older",
      task_id: "task-root",
      status: "completed",
      created_at: 10,
      thread_id: "thread-root",
    };
    const newer = {
      id: "root-newer",
      task_id: "task-root",
      status: "in_progress",
      created_at: 20,
      thread_id: "thread-root",
    };

    const forward = deriveSpawnedAgentTree([older, newer], "thread-root");
    const reversed = deriveSpawnedAgentTree([newer, older], "thread-root");

    expect(forward?.anchor?.item.id).toBe("root-newer");
    expect(reversed?.anchor?.item.id).toBe("root-newer");
  });
});
