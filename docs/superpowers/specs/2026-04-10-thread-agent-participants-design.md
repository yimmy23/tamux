# Thread Agent Participants Design

Date: 2026-04-10
Status: Proposed

## Summary

This design introduces two separate operator-facing agent command families in the TUI:

- `!agent ...` for hidden internal delegation
- `@agent ...` for visible resident thread participants

The current main thread owner remains unchanged when participants are added. Participants watch the thread continuously, decide when their assigned task is relevant, and contribute through the existing queued-message flow so the operator can use the current `send now` behavior to interrupt the main agent when needed.

## Goals

- Keep `!agent` as an internal, hidden coordination path.
- Add `@agent` as a persistent in-thread participant registration mechanism.
- Allow multiple active participants per thread.
- Preserve the current main thread owner and its ongoing work.
- Reuse the existing queued-message and `send now` flow for participant contributions.
- Make the command behavior discoverable in dedicated documentation and from `README.md`.

## Non-Goals

- Replacing the existing main-thread ownership model.
- Turning participants into equal unmanaged responders with no queue arbitration.
- Conflating `!agent` delegation with `@agent` participation.
- Changing file reference syntax outside the leading command position.

## Operator Command Model

### `!agent ...`

`!agent ...` remains the hidden internal delegation path.

Expected behavior:

- Delegates work to the target agent using internal coordination state.
- May use hidden internal DM or handoff plumbing.
- Does not register a visible participant on the current thread.
- Does not make the target agent a persistent visible responder in the main thread.

### `@agent ...`

`@agent ...` registers or updates a visible participant on the current thread.

Expected behavior:

- Keeps the current main thread owner unchanged.
- Creates or updates a participant assignment for the named agent.
- The participant remains active until explicitly removed.
- The participant continuously observes thread activity and decides whether to contribute based on its assigned task.
- When the participant decides to contribute, its message enters the thread through the existing queued-message path.

Example:

```text
@weles verify claims and jump in when something looks wrong
@rarog focus on implementation risks and performance regressions
```

### Participant Control Phrases

The following should deactivate a participant without changing thread ownership:

- `@agent leave`
- `@agent stop`
- `@agent done`
- `@agent return`

These control phrases should be normalized to a single participant-deactivation intent.

## Parsing Rules

The TUI should treat only the leading token specially.

Rules:

- If the prompt begins with `@<known-agent-alias>`, parse it as a participant command.
- If the prompt begins with `!<known-agent-alias>`, parse it as an internal delegation command.
- All later `@...` tokens in the prompt continue to participate in file-reference parsing.
- If the leading `@token` is not a known agent alias, preserve current file-reference behavior.

Example:

```text
@weles inspect @crates/amux-daemon/src/agent/messaging.rs
```

Interpretation:

- leading `@weles` => participant command
- later `@crates/...` => file reference

## Thread Data Model

Each thread should gain a participant registry separate from the thread owner and separate from handoff state.

Suggested participant fields:

- `agent_id`
- `agent_name`
- `instruction`
- `status` (`active` or `inactive`)
- `created_at`
- `updated_at`
- `deactivated_at`
- `last_contribution_at`
- optional runtime metadata for future heuristics

This registry should live in daemon-managed thread metadata and persist across reloads.

## Runtime Behavior

### Thread Owner

- The main thread owner remains responsible for the normal thread lifecycle.
- Adding or removing participants does not transfer thread ownership.

### Participant Observation

- Active participants receive ongoing visibility into thread activity.
- Each participant evaluates whether the conversation is relevant to its assigned task.
- Participants should be allowed to remain silent unless they judge their contribution useful.

### Participant Contribution Path

- Participant contributions should appear as direct, visible agent messages in the main thread.
- Contributions should enter through the existing queued-message mechanism.
- The operator should retain the current `send now` option to preempt the main agent if the queued participant contribution is more urgent.

### Deactivation

- Deactivation removes the participant from active observation for that thread.
- Deactivation should not inject a bogus operator message into the thread.
- Deactivation should not interrupt the main thread owner unless the queue system already requires it.

## Daemon Responsibilities

- Parse and normalize thread participant commands coming from the TUI.
- Maintain per-thread participant state.
- Fan out thread activity to active participants.
- Accept participant-originated queued contributions into the main thread queue.
- Reuse current queue arbitration and `send now` behavior.
- Keep `!agent` internal delegation on its existing hidden path.

## TUI Responsibilities

- Detect leading `@agent` and `!agent` directives.
- Preserve current file-reference behavior for non-leading `@...` tokens.
- Display participant contributions as normal queued thread contributions.
- Add future discoverability affordances if needed, but no new UI is required for the first implementation beyond command parsing and existing queue rendering.

## Persistence

Participant state should survive:

- thread reloads
- daemon restarts
- TUI reconnects

This implies storing participant metadata alongside existing thread metadata or in a closely related persisted structure.

## Error Handling

- Unknown leading agent alias: fall back to current plain prompt or file-reference behavior.
- `@agent leave` for an inactive or missing participant: return a non-fatal status message.
- Duplicate `@agent ...` registration: update the participant’s instruction rather than creating duplicates.
- If participant execution fails internally, surface it through existing queue or audit paths rather than silently mutating thread ownership.

## Testing

Add tests for:

- leading `@agent` parsing with later file refs
- `!agent` staying on the hidden internal path
- participant registration on an existing thread
- participant update semantics on repeated `@agent ...`
- participant deactivation phrases
- persistence and reload of participant metadata
- participant contributions entering the normal queued-message path
- `send now` still interrupting the main agent correctly when a participant contribution is queued

## Documentation

Implementation should add:

- a dedicated user-facing behavior document describing `!agent` and `@agent`
- a pinned link from `README.md` to that document

This design document is the implementation reference; the follow-up user-facing document should be shorter and task-oriented.

## Recommended Implementation Order

1. Add daemon thread-participant data model and persistence.
2. Add TUI parsing for leading `@agent` and keep `!agent` on the internal path.
3. Register and deactivate participants from TUI commands.
4. Wire participant observation and queued contributions.
5. Add documentation and `README.md` pin.
6. Run targeted tests for TUI parsing, persistence, and queue behavior.

## Open Questions

No blocking open questions remain for the initial implementation.
