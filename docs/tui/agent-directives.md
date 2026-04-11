# Agent Directives

tamux supports two agent-targeting prefixes in chat input.

## Hidden Delegation

Use `!agent ...` for hidden internal delegation.

Examples:

- `!weles verify whether this claim is correct`
- `!rarog prepare a softer operator-facing summary`

Behavior:

- the current visible thread stays where it is
- the request is sent internally to the target agent
- the current thread context is attached when you issue it from an existing thread

## Thread Participants

Use `@agent ...` to register or update a resident participant on the current thread.

Examples:

- `@weles verify claims before answering`
- `@rarog jump in when the operator looks confused`

Behavior:

- the main thread owner does not change
- repeating `@agent ...` updates the participant instead of creating duplicates
- participants stay registered until you stop them

## Stopping A Participant

Use one of these forms on an existing thread:

- `@weles stop`
- `@weles leave`
- `@weles done`
- `@weles return`

These deactivate that participant without changing the thread owner.

## File References

Leading `@agent` is treated as an agent directive when it matches a known agent alias.

File references still work everywhere else:

- `Review @src/main.rs`
- `@weles inspect @crates/amux-daemon/src/server/dispatch_part3.rs`
