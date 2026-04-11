# Thread Agent Participants Design

Date: 2026-04-10
Status: Proposed

## Summary

This design introduces two separate operator-facing agent command families in the TUI:

- `!agent ...` for hidden internal delegation
- `@agent ...` for visible resident thread participants

The current main thread owner remains unchanged when participants are added. Participants watch the thread continuously, decide when their assigned task is relevant, and contribute through a staged suggestion queue so the operator can use the existing `send now` behavior to interrupt the main agent when needed. Participants can also mark a suggestion as forced to preempt the main stream immediately.

## Goals

- Keep `!agent` as an internal, hidden coordination path.
- Add `@agent` as a persistent in-thread participant registration mechanism.
- Allow multiple active participants per thread.
- Preserve the current main thread owner and its ongoing work.
- Stage participant contributions through a per-thread suggestion queue with `send now`.
- Allow participants to force-send a suggestion when they believe interruption is required.
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
- When the participant decides to contribute, it produces a staged suggestion that the operator can send or dismiss.
- If the suggestion is marked forced, it should interrupt the main agent and post immediately.

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

Known-agent alias resolution:

- Use the daemon-provided agent registry (same list used today for `!agent` completion).
- Aliases are case-insensitive and normalized to lowercase for lookup.
- If multiple aliases map to the same agent, the canonical agent name is stored on the participant record.

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

### Participant Suggestion Queue

Each thread should also maintain a suggestion queue for participant contributions. Suggested fields:

- `suggestion_id`
- `thread_id`
- `participant_agent_id`
- `participant_agent_name`
- `content`
- `created_at`
- `force_send` (boolean)
- `status` (queued, sent, dismissed, failed)

Suggestions are not thread messages until they are sent (either by operator action or force-send). They should be persisted in the daemon (preferably via agent events) so they survive reconnects.

Arbitration order:

- Forced suggestions preempt any active main-agent stream and post immediately.
- Normal queued suggestions are FIFO and only posted via operator `send now`.
- If multiple forced suggestions arrive at once, process them in arrival order.

## Runtime Behavior

### Thread Owner

- The main thread owner remains responsible for the normal thread lifecycle.
- Adding or removing participants does not transfer thread ownership.

### Participant Observation

- Active participants receive ongoing visibility into thread activity.
- Each participant evaluates whether the conversation is relevant to its assigned task.
- Participants should be allowed to remain silent unless they judge their contribution useful.

Visibility scope:

- Participants receive the same thread message stream as the main agent, excluding hidden internal delegation content and system-only audit events.
- Participant suggestions are not fed back into other participants unless they are sent and become normal thread messages.

### Participant Contribution Path

- Participants emit staged suggestions that appear in the queue, not immediately as thread messages.
- The operator can `send now` or dismiss queued suggestions.
- If a suggestion is marked `force_send`, it should interrupt the main agent stream and post immediately as that participant.
- Posted messages must record the participant as the author, not the main thread owner.

Force-send semantics:

- If a participant marks `force_send`, the daemon cancels the current main-agent stream (same mechanism as `send now`) and immediately posts the participant message.
- If no stream is active, the suggestion posts immediately without cancellation.

### Deactivation

- Deactivation removes the participant from active observation for that thread.
- Deactivation should not inject a bogus operator message into the thread.
- Deactivation should not interrupt the main thread owner unless the queue system already requires it.

## Daemon Responsibilities

- Validate and apply thread participant commands coming from clients.
- Maintain per-thread participant state.
- Fan out thread activity to active participants.
- Accept participant-originated suggestions into the per-thread queue.
- Reuse current queue arbitration and `send now` behavior for staged suggestions.
- Allow participants to flag a suggestion as forced and interrupt the main stream.
- Keep `!agent` internal delegation on its existing hidden path.

## TUI Responsibilities

- Detect leading `@agent` and `!agent` directives.
- Preserve current file-reference behavior for non-leading `@...` tokens.
- Display participant suggestions in the queue with agent name and force-send indicator.
- Allow `send now` and dismiss for queued participant suggestions.
- Add future discoverability affordances if needed, but no new UI is required for the first implementation beyond command parsing and queue rendering updates.

## React And Electron Responsibilities

The desktop frontend should support the same behavior model as the TUI, not a separate agent-command dialect.

### Composer Parsing

- Detect leading `@agent` and `!agent` directives in the React chat composer.
- Preserve current file-reference and attachment behavior for later `@...` tokens in the same prompt.
- Use the same known-agent alias resolution rules as the TUI.

### Thread Participant UX

- Show active thread participants in thread detail or header UI.
- Make it clear that participants are additive and do not replace the main thread owner.
- Show participant state at minimum as:
  - agent name
  - active or inactive
  - assigned instruction
- Allow future editing and removal from the UI, but command-driven registration is sufficient for the first implementation.

### Contribution UX

- Participant contributions should render as staged suggestions in the queue with agent name and force-send indicator.
- The current `send now` mechanism should work the same way for participant-originated queued suggestions as it does for any other queued message.
- If the main agent is streaming and a participant suggestion is queued, the React UI should surface the same interruption affordance the TUI already exposes conceptually.

### Transport

- The Electron preload and IPC layer should expose the same command primitives as the TUI client path:
  - internal delegation via `!agent`
  - participant registration and deactivation via `@agent`
- The renderer should not duplicate daemon decision logic. It should only parse commands and send normalized intents.

### Shared Backend Contract

- React and TUI should converge on the same daemon-facing API for:
  - register participant
  - update participant
  - deactivate participant
  - internal delegation
- The daemon remains the source of truth for participant state, queue state, and contribution arbitration.

## Persistence

Participant state should survive:

- thread reloads
- daemon restarts
- TUI reconnects

This implies storing participant metadata alongside existing thread metadata or in a closely related persisted structure.

Participant suggestions should survive reconnects and daemon restarts by persisting the queue in agent events or adjacent durable storage.

Durability guarantee: suggestions and participant state should be recoverable after a daemon restart and visible to clients after reconnect without requiring operator re-entry.

## Error Handling

- Unknown leading agent alias: fall back to current plain prompt or file-reference behavior.
- `@agent leave` for an inactive or missing participant: return a non-fatal status message.
- Duplicate `@agent ...` registration: update the participant’s instruction rather than creating duplicates.
- If participant execution fails internally, surface it through existing queue or audit paths rather than silently mutating thread ownership.
- If suggestion posting fails, keep it queued and mark it failed with a retry affordance.
- If force-send fails, downgrade it to a failed queued suggestion and allow manual retry.

## Authorization

- Adding/removing participants and sending suggestions requires the same operator permissions as normal message sending.
- Participants can force-send regardless of operator interruption preferences.

## Testing

Add tests for:

- leading `@agent` parsing with later file refs
- `!agent` staying on the hidden internal path
- participant registration on an existing thread
- participant update semantics on repeated `@agent ...`
- participant deactivation phrases
- persistence and reload of participant metadata
- participant contributions entering the normal staged suggestion path
- `send now` still interrupting the main agent correctly when a participant suggestion is queued
- force-send suggestions interrupting the main agent stream
- React composer parsing and intent normalization for `@agent` and `!agent`
- Electron IPC transport coverage for participant commands
- React thread UI rendering of active participants and participant contributions

## Documentation

Implementation should add:

- a dedicated user-facing behavior document describing `!agent` and `@agent`
- a pinned link from `README.md` to that document

This design document is the implementation reference; the follow-up user-facing document should be shorter and task-oriented.

## Recommended Implementation Order

1. Add daemon thread-participant data model and persistence.
2. Add shared daemon-facing command primitives for internal delegation and participant management.
3. Add TUI parsing for leading `@agent` and keep `!agent` on the internal path.
4. Add React/Electron composer parsing and IPC transport for the same command intents.
5. Register and deactivate participants from both TUI and React flows.
6. Wire participant observation and staged suggestions.
7. Add participant display in the React thread UI.
8. Add documentation and `README.md` pin.
9. Run targeted tests for TUI parsing, React parsing, persistence, IPC transport, and queue behavior.

## Open Questions

No blocking open questions remain for the initial implementation.
