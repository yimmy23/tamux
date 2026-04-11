# Thread Participant Routing Fix Design

## Summary

Thread-visible participants and hidden internal delegation must use different execution paths.

- `@agent` means visible thread participation.
- `!agent` means hidden internal DM/delegation.

The current participant observer path reuses hidden internal messaging. That violates the intended routing contract and creates a plausible re-entry path during normal thread execution. This fix removes hidden internal messaging from `@agent` participation while preserving `!agent` internal delegation.

## Goals

- Keep `@agent` participation fully visible on the thread.
- Preserve queued participant suggestions for non-forced participant output.
- Preserve hidden internal delegation for `!agent`.
- Prevent participant execution from creating hidden DM threads or `internal_delegate` messages.
- Reduce the chance of recursive runtime execution during active thread work.

## Non-Goals

- Reworking the entire multi-agent scheduler.
- Changing the `!agent` internal delegation transport.
- Redesigning participant prompt syntax beyond the existing `FORCE` and `MESSAGE` response format.

## Routing Contract

### `@agent`

`@agent` is a thread actor.

- It reads visible thread context.
- `FORCE: yes` creates a visible assistant message on the same thread.
- `FORCE: no` queues a visible participant suggestion for operator review/send.
- It must not create or depend on hidden DM traffic.

### `!agent`

`!agent` is a hidden delegate.

- It uses the existing internal DM path.
- It must not silently appear as a visible thread participant.
- It remains separate from thread participant registration and observer execution.

## Proposed Changes

### 1. Split participant execution from internal delegation

Thread participant observers will no longer call `send_internal_agent_message`.

Instead, observer execution will:

1. Build the participant prompt from visible thread messages.
2. Obtain the participant decision without creating internal DM thread artifacts.
3. Route the result directly:
   - `FORCE: yes` -> append a visible thread message authored by the participant, bypassing suggestion creation entirely.
   - `FORCE: no` -> queue a participant suggestion.

Internal delegate helpers remain available only for `!agent`.

### 2. Add a visible participant-post path

The daemon will have an explicit helper for visible participant posts.

Requirements:

- Writes an assistant message directly into the target thread.
- Sets `author_agent_id` and `author_agent_name`.
- Updates participant contribution timestamps.
- Persists the thread and emits reload events.
- Never leaves hidden system or `internal_delegate` messages behind.

### 3. Keep observer prompting scoped to visible context

Participant prompts will continue excluding:

- system messages,
- hidden/internal delegate messages,
- other non-visible transport artifacts.

This keeps `@agent` participants grounded in visible thread discussion only.

Observer execution must also avoid immediate self-feedback. A participant-generated visible post must not be re-consumed by the same participant as a fresh trigger in the same observer cycle.
One observer cycle must use a single immutable visible-thread snapshot for all participants so participant order cannot create same-cycle cascades.

### 4. Fail closed on visible participant execution

If visible participant posting fails:

- emit a workflow notice,
- keep the main user send successful,
- do not fall back to hidden internal messaging.

This preserves the routing contract and avoids masking failures.

## Failure Handling

- Empty participant response: no action.
- `FORCE: no` with content: queue suggestion.
- `FORCE: yes` with content: visible participant post and no suggestion record.
- Visible post failure: workflow notice, no hidden fallback, no outer-send failure.
- Suggestion queue failure: return error, no alternate route.

## Testing

Add or update tests for the following:

1. `@agent` participant observer with `FORCE: yes` appends a visible assistant message on the thread and tags it with the participant identity.
2. `@agent` participant observer with `FORCE: yes` does not create a hidden DM thread.
3. `@agent` participant observer with `FORCE: no` queues a suggestion and does not create a hidden DM thread.
4. Visible participant execution does not leave `internal_delegate` messages on the visible thread.
5. `!agent` internal delegation still uses the hidden DM path and does not register a visible participant.
6. A participant visible post does not immediately retrigger the same participant within the same observer cycle.
7. `FORCE: yes` does not create queued or failed participant suggestion records.
8. A multi-participant observer pass uses one immutable thread snapshot so earlier participant output does not affect later participant prompts in the same cycle.
9. Participant observer failure emits a workflow notice without failing the outer user send.

## Implementation Notes

- Keep the change narrow to daemon participant execution paths.
- Do not change unrelated skill-mesh or DIDSM behavior in the same patch.
- Prefer adding a dedicated visible participant helper over reusing internal delegate helpers with conditionals.
