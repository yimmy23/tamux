# Goals — Durable goal runners for multi-step autonomous objectives

## Agent Rules

- **Use goals for complex, multi-step objectives** — goals automatically plan, execute child tasks, handle failures, replan, and reflect
- **Keep `goal` descriptions clear and outcome-focused** — the daemon's agent decomposes goals into executable steps
- **Monitor goal progress with `get_goal_run`** — check current step, status, and events
- **Use `control_goal_run` to intervene** — pause, resume, cancel, or retry specific steps
- **Don't micromanage goals** — the goal runner handles planning, approval, retries, and replanning automatically
- **Goals survive daemon restarts** — they are persisted in SQLite
- **Goals can spawn child tasks** — each step becomes a queued task in the daemon task queue
- **Bind a goal to a session** — use `session_id` to ensure all child tasks execute in the right terminal

## Reference

### Tool: `start_goal_run`

**Description:** Start a durable goal run that plans, executes child tasks, handles approvals, reflects, and optionally generates reusable skills.

| Param | Type | Required | Description |
|---|---|---|---|
| `goal` | string | Yes | The objective to accomplish (e.g., "Fix the failing CI pipeline and ensure all tests pass") |
| `title` | string | No | Short display title |
| `thread_id` | string | No | Existing agent thread to use for context |
| `session_id` | string | No | Target terminal session UUID |
| `priority` | string | No | `low`, `normal`, `high`, `urgent` |

**Returns:** Full goal_run JSON object.

### Goal Run Lifecycle

```
Queued -> Planning -> Running -> Completed
                          \-> AwaitingApproval (child task needs approval)
                          \-> Failed (can replan up to max_replans)
                          \-> Paused (via control_goal_run)
Any -> Cancelled
```

**Planning phase:** The agent analyzes the goal and generates a structured plan with steps.

**Step types:**

| Kind | Purpose |
|---|---|
| `reason` | Think through a problem, analyze data |
| `command` | Execute a shell command |
| `research` | Search history, read files, gather info |
| `memory` | Update persistent memory |
| `skill` | Generate a reusable skill document |

**Step statuses:** `pending`, `in_progress`, `completed`, `failed`, `skipped`

### Tool: `list_goal_runs`

**Description:** List all goal runs with status, current step, and metrics.

**Parameters:** None

**Returns:** Array of goal_run objects with: id, title, goal, status, priority, current_step_index, current_step_title, replan_count, child_task_count, approval_count, duration_ms.

### Tool: `get_goal_run`

**Description:** Fetch a specific goal run with full plan, events, and derived metrics.

| Param | Type | Required | Description |
|---|---|---|---|
| `goal_run_id` | string | Yes | Goal run UUID |

**Returns:** Complete goal_run with:

- `steps[]` — full plan with per-step status, summary, errors, timing
- `events[]` — chronological log of all actions taken
- `memory_updates` — what the agent learned
- `generated_skill_path` — if a skill was generated
- `reflection_summary` — agent's post-completion reflection
- `plan_summary` — overview of the generated plan

### Tool: `control_goal_run`

**Description:** Control goal run lifecycle or rerun specific steps.

| Param | Type | Required | Description |
|---|---|---|---|
| `goal_run_id` | string | Yes | Goal run UUID |
| `action` | string | Yes | `pause`, `resume`, `cancel`, `retry_step`, `rerun_from_step` |
| `step_index` | integer | No | Required for `retry_step` and `rerun_from_step` |

**Returns:** `{ goal_run_id, action, step_index, ok: true/false }`

### Example: Autonomous Investigation

```
Start: goal="Investigate why nightly Rust CI is failing, identify root cause, apply minimal fix"

Goal runner automatically:
1. Plans: [research git log -> read CI config -> run failing test -> analyze error -> apply fix -> verify]
2. Executes each step as a child task
3. If step 5 fails -> replans with new approach
4. On success -> reflects on what worked, updates memory, generates skill
```

## Gotchas

- Goals use the daemon's built-in agent — they consume LLM tokens for planning and reflection
- `max_replans` limits how many times a goal can recover from failure (default varies)
- Approval gates: if a child task triggers the policy engine, the entire goal pauses in `awaiting_approval`
- Goals generate events during execution — use `get_goal_run` to watch progress
- `rerun_from_step` re-executes from a specific step index onward, keeping earlier results
- Thread context: if you provide `thread_id`, the goal runner uses that conversation's history for context
