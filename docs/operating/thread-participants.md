# Thread Participants

Thread participants let you keep specialist agents attached to a conversation without changing the main thread owner.

Use them when you want the main agent to keep driving the thread, but you also want another agent to watch for a specific class of issue and jump in when it matters.

## Hidden Delegation vs Participants

Use `!agent ...` for hidden delegation.

Examples:

- `!weles verify whether this claim is correct`
- `!rarog prepare a softer operator-facing summary`

Hidden delegation does not register a visible participant on the thread. It stays on the internal path.

Use `@agent ...` for visible resident participants.

Examples:

- `@weles verify claims before answering`
- `@rarog jump in when implementation risks or regressions appear`

Participant registration keeps the main thread owner unchanged. Repeating `@agent ...` updates the assignment instead of creating duplicates.

## Stopping A Participant

These commands all deactivate the participant without changing thread ownership:

- `@weles stop`
- `@weles leave`
- `@weles done`
- `@weles return`

The participant stays in thread history as inactive state, but it stops observing new thread activity.

## How Suggestions Work

Active participants observe visible thread activity and decide whether they have something useful to add.

When a participant wants to contribute, the daemon places that contribution into the queued suggestion flow instead of immediately replacing the main agent's response.

Each suggestion includes:

- the participant agent name
- the suggested message text
- whether the suggestion is marked for force-send
- whether the suggestion is queued or failed

Failed suggestions stay visible so you can retry them.

## Force-Send

Some participant suggestions are marked as force-send.

Force-send means:

- the suggestion is urgent enough to interrupt an active stream
- sending it will stop the current stream first
- the participant message is then posted as a visible assistant message authored by that participant

In the TUI and desktop UI, force-send suggestions are clearly marked before you send them.

## Visibility Rules

Participants do not see everything.

The participant observer prompt excludes:

- hidden internal delegation content
- participant suggestion queue contents
- system-only coordination that should not feed back into participant observation

This prevents suggestion loops and keeps participant reasoning tied to the visible thread.

## UI Surfaces

Current UI behavior:

- the TUI shows queued participant suggestions in the queued message flow
- the desktop chat panel shows active participants in the thread header
- the desktop chat panel shows queued participant suggestions inline with send and dismiss actions
- participant-authored assistant messages are labeled with the agent name

## Related Docs

- [Agent Directives](../tui/agent-directives.md)
- [How tamux Works](../how-tamux-works.md)