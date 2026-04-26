# Workspaces

Workspaces are tamux's board-level planning and execution layer. They turn loose agent work into a visible Jira-style flow where every item has an owner, a status, a review path, and a durable audit trail.

The short version:

- a **thread** is a persisted conversation and tool-use surface
- a **goal** is a durable autonomous orchestration run
- a **workspace task** is a board-owned work item that targets either a thread or a goal
- a **queue entry** is lower-level daemon execution machinery used by goals, agents, and managed commands

Only workspace-owned cards should be called tasks in operator docs. Threads and goals are the execution targets; the workspace task is the planning and review wrapper around them.

## What A Workspace Gives You

Workspaces add structure around long-running agent work:

- a shared board with `Todo`, `In Progress`, `In Review`, and `Done`
- explicit task fields for title, type, description, definition of done, priority, assignee, reviewer, and reporter
- one place to run, pause, stop, delete, assign, review, and reopen work
- a clear distinction between user-driven work and Svarog-operated automatic work
- history for every thread or goal run attached to a task
- a local workspace mirror that other apps can read

This matters because threads and goals are good execution surfaces, but they are not enough by themselves when the operator needs planning, review, ownership, and status across many pieces of work.

## Task Fields

Each workspace task has:

- **Title**: required
- **Task type**: `thread` or `goal`, required at creation
- **Description**: required
- **Definition of done**: optional success criteria
- **Priority**: defaults to `low`
- **Assignee**: optional during creation, required before run
- **Reviewer**: user, Svarog, or another configured agent/subagent
- **Reporter**: automatically set to the user or Svarog
- **Status**: one of `Todo`, `In Progress`, `In Review`, `Done`
- **History**: previous and current thread or goal runs, newest first
- **Deletion state**: soft-deleted with `deleted_at`

Creating a task reserves the target `thread_id` or `goal_id` immediately. The target is not actually run until the task is run, dragged into `In Progress`, or picked up by the workspace operator.

## Board Lifecycle

New workspace tasks land at the bottom of `Todo`.

When a task runs:

1. The workspace checks that the task has an assignee.
2. The reserved thread or goal is started with the task title, description, definition of done, and workspace context.
3. The task moves to `In Progress`.
4. The assignee works in the target thread or goal.
5. Completion sends a workspace task-completion event.

After completion:

- if there is no reviewer, the task can move to `Done`
- if the reviewer is an agent or subagent, the workspace creates or starts a review target automatically
- if the reviewer is the user, the task waits in `In Review` for a user action

Review is not a rerun of the original task. The review target receives the task id, the original task details, the completion summary, and the definition of done. Its job is to decide whether the delivered work satisfies the task.

## Failed Review Loop

If review fails:

1. The review result is written under the workspace directory as `task-<id>/failed-review.md`.
2. The task moves back to `In Progress`.
3. The failure notes are attached to the next assignee prompt.
4. A new thread or goal is created with the same task settings plus a link to the failed review.
5. The new target becomes the primary `Open` target.
6. Older targets remain available from task `History`, newest first.

This keeps the current card clean while preserving the evidence trail. The operator can inspect what was attempted, why it failed, and which target is now active.

## Operator Mode

Every workspace has an operator setting:

- **User**: task transitions and runs are explicit user actions
- **Svarog**: the workspace may operate automatically where policy and task state allow

The switch is high-level workspace state, not a per-task display toggle. It exists because the same board may need to run as a user-dependent planning board in one phase and as an automatically operated Svarog board in another.

Svarog has access to all workspace tools. Other agents and subagents get readable workspace tools unless the runtime explicitly grants more.

## Persistence And Local Mirror

Workspace state lives in the daemon database and is also mirrored to disk:

```text
<tamux install dir>/workspaces/workspace-<id>/
  workspace.json
  task-<id>/
    failed-review.md
```

`workspace.json` contains the workspace metadata, operator setting, tasks, statuses, assignees, reviewers, history, and other board state. The local mirror makes workspace state easy to inspect from editors, scripts, and external apps without asking the TUI to be the only view.

SQLite remains the authoritative structured store. The file mirror is a durable local copy for visibility and integration.

## When To Use Workspaces

Use a workspace when:

- you have multiple related threads or goals to coordinate
- work needs review before it is considered done
- you want tasks to move across a visible board
- failures should produce an explicit retry loop
- Svarog should be able to operate the backlog automatically
- another tool should track the board through `workspace.json`

Use a standalone thread when the work is conversational and does not need board ownership. Use a standalone goal when the work needs durable autonomy but does not need workspace-level assignment, review, and status management.

## Related Reading

- [How tamux Works](how-tamux-works.md)
- [Goal Runners](goal-runners.md)
- [Best Practices](best-practices.md)
- [Thread Participants](operating/thread-participants.md)
