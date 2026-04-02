# Goal Runners

Goal runners are tamux's durable autonomy layer. Instead of a single prompt/response exchange, you give the daemon a long-running objective and let it plan, execute, replan, reflect, and persist what it learns.

![Goal runner operator view](assets/goal-runner-overview.svg)

## What A Goal Run Does

1. Accepts a long-running objective from the UI.
2. Uses the built-in daemon agent to generate a structured plan.
3. Converts plan steps into child tasks on the daemon task queue.
4. Watches approvals, retries, failures, and completion.
5. Reflects on the final trajectory.
6. Optionally writes durable memory and generates a reusable skill document.

## Starting A Goal Run

From the desktop app:

1. Open the agent panel.
2. Confirm the backend is set to `daemon`.
3. Open `Goal Runners`.
4. Enter a goal prompt.
5. Optionally provide a title and target session ID.
6. Start the run.

Good goal prompts are specific, bounded, and outcome-oriented.

Example:

```text
Investigate why the nightly Rust build is failing, identify the root cause, propose the smallest safe fix, and capture any reusable workflow as a skill.
```

## Lifecycle

Goal runs move through these top-level states:

- `queued`: accepted by the daemon but not planned yet
- `planning`: the daemon is building the initial structured plan
- `running`: the goal is executing or waiting on child-task progress
- `awaiting_approval`: a child task hit a managed-command approval gate
- `paused`: orchestration is paused by the operator
- `completed`: all steps finished and the daemon recorded the final reflection
- `failed`: the run exhausted replanning or failed irrecoverably
- `cancelled`: the operator cancelled the goal run

Step-level status is tracked separately inside the run:

- `pending`
- `in_progress`
- `completed`
- `failed`
- `skipped`

## How Goal Runs Use The Task Queue

Goal runners do not replace the daemon task queue. They sit above it.

- A goal run owns the long-lived objective and plan.
- Each executable step becomes a child task.
- Child tasks run through the existing queue, lane, approval, and retry machinery.
- The goal runner watches those child tasks and decides whether to continue, replan, or fail.

This means goal runners inherit the same safety controls as the rest of tamux:

- managed terminal execution
- approval gating
- snapshots
- queue visibility
- lane and workspace scheduling

## Approvals And Replanning

When a child task hits an approval boundary:

- the task enters `awaiting_approval`
- the goal run surfaces `awaiting_approval`
- execution resumes only after the operator resolves the approval request

When a child task fails:

- the goal runner records the failure
- if replanning budget remains, it asks the daemon agent for revised remaining steps
- if replanning budget is exhausted, the goal run becomes `failed`

## Memory And Skill Output

On successful completion, tamux can produce two durable outputs:

- **Memory update**: appended to `MEMORY.md` only when the reflection identifies a stable fact or operator preference worth preserving
- **Generated skill**: a reusable workflow document derived from the successful trajectory

The structured run history stays in SQLite. The editable memory and skill artifacts remain on disk.

## Current Limits

- Goal runners currently require the built-in `daemon` backend.
- `pause` stops future orchestration but does not forcibly terminate a child task that is already running.
- Reflection-driven memory updates are intentionally conservative and should capture durable knowledge, not temporary run output.

## Suggested Operator Workflow

Use goal runners when:

- the task will take multiple steps
- you want child task visibility and approvals
- you want the daemon to survive UI disconnects
- the result may be reusable as durable memory or a procedural skill

Use a normal chat turn when:

- you only need reasoning or a quick answer
- no durable execution loop is required
- the task does not need queueing, replanning, or operator checkpoints
