# Routine workflows

This slice productizes routines as the durable habit object behind daily briefs, digests, monitors, and recurring work.

## What a routine now supports

- versioned routine schema validation (`schema_version: 1`)
- early rejection of invalid cron strings, invalid notify channels, malformed launch assignments, and unsupported target kinds
- `preview_routine` dry-run showing next fire times, materialized payload, delivery fan-out, and approval posture without mutation
- persisted run history for scheduled runs, run-now executions, and reruns
- `update_routine`, `run_routine_now`, `list_routine_history`, and `rerun_routine` controls
- pause, resume, delete, and detailed get/list flows with last-result summary state

## Create a routine

Task-backed routine:

```json
{
  "title": "Daily brief",
  "description": "Prepare a project brief every morning",
  "schedule_expression": "0 9 * * *",
  "target_kind": "task",
  "target_payload": {
    "title": "Prepare daily brief",
    "description": "Summarize project status and blockers",
    "priority": "high",
    "notify_on_complete": true,
    "notify_channels": ["slack", "telegram"]
  }
}
```

Goal-backed routine:

```json
{
  "title": "Weekly research digest",
  "description": "Kick off the weekly digest goal",
  "schedule_expression": "0 8 * * 1",
  "target_kind": "goal",
  "target_payload": {
    "goal": "Produce the weekly research digest and capture reusable findings",
    "title": "Weekly research digest",
    "priority": "normal",
    "requires_approval": false
  }
}
```

Validation happens before save. If the cron string is invalid, the target kind is unsupported, notify channels are outside `in-app|slack|discord|telegram|whatsapp`, or required target payload fields are missing, the create/update call fails immediately.

## Preview / dry-run

Use `preview_routine` before trusting a routine:

```json
{
  "routine_id": "routine-daily-brief",
  "fire_count": 3
}
```

The response includes:

- `next_fire_times`
- `materialized_payload`
- `delivery_fan_out`
- `approval_posture`
- `would_mutate_state: false`
- `would_enqueue_work: true`

Preview does **not** enqueue tasks, create goal runs, or mutate routine state.

## Run now

Use `run_routine_now` to execute one stored routine immediately:

```json
{
  "routine_id": "routine-daily-brief"
}
```

This records a routine run with `trigger_kind = run_now` and returns any created task or goal run IDs.

## History and rerun

History:

```json
{
  "routine_id": "routine-daily-brief",
  "limit": 10
}
```

Rerun a prior attempt from its last materialized payload:

```json
{
  "run_id": "routine-run-..."
}
```

Reruns create a new history record with `trigger_kind = rerun` and `rerun_of_run_id` pointing at the original run.

## Update / recovery

- `update_routine` edits a routine in place with the same validation used at creation.
- `pause_routine` stops due materialization.
- `resume_routine` clears the pause and preserves an already-due next fire time when appropriate.
- `delete_routine` retires the definition without raw DB edits.
- `get_routine` returns the routine plus recent persisted history and last-result summary fields.

## Recovery guidance

If a routine preview or execution fails:

1. inspect `last_error` and `list_routine_history`
2. fix the invalid schedule or payload using `update_routine`
3. re-run `preview_routine`
4. use `run_routine_now` or `rerun_routine` once the preview looks correct

## Focused verification

Run these after changing the routine layer:

```bash
cargo test -p zorai-daemon routine
cargo test -p zorai-daemon materialized_routine
```

## Proof flow

The proof for this slice is:

```text
create -> preview -> run-now -> inspect history -> rerun -> pause -> resume
```
