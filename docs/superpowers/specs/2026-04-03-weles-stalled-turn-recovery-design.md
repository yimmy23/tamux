# WELES Stalled Turn Recovery Design

## Goal

Detect agent turns that ended unexpectedly in an unfinished state, let WELES evaluate whether the turn was actually complete, and automatically resume the thread or goal run when the agent likely stopped before doing the action it said it would do.

## Scope

- Add daemon-side stalled-turn supervision for all threads and goal runs.
- Wait before intervention so active conversations and long-running work are not interrupted prematurely.
- Distinguish healthy waiting states from unhealthy stopped-turn states.
- Let WELES classify likely unfinished turns and send internal continuation nudges.
- Retry automatic recovery three times with increasing delays.
- Persist supervision evidence so the daemon can learn which message patterns are strong predictors of unfinished turns.
- Surface hard-failed recovery as a stronger stuck state for operators and future automation.

## Non-Goals

- Do not treat active tool execution or waiting on a tool result as stalled.
- Do not interrupt healthy streams that are still live or retrying inside the normal LLM/tool loop.
- Do not redesign the existing approval system, goal planner, or gateway routing model.
- Do not require the frontend or TUI to be connected for recovery to happen.
- Do not replace provider-level retry logic already implemented in the send loop.

## Problem Statement

The daemon already handles streamed turns, tool loops, retries, and general liveness checks, but it does not yet supervise a specific class of failure:

- the agent writes a progress or promise message such as "Excellent. Let me start drafting..." or "Working. Let me produce the redesign now."
- no tool call, artifact, follow-up assistant content, or task/goal progress follows
- the stream has already ended unexpectedly, so the normal loop is no longer active
- from the operator's perspective, the thread appears alive for a moment and then silently dies in an unfinished state

This is different from a slow task or a long-running command. The failure happens after the agent has effectively committed to a next action but does not perform it.

## Current State

The daemon already contains several pieces that this feature should reuse instead of replacing:

- The internal send loop tracks active stream lifecycles, retry states, and unexpected stream endings.
- Threads persist full assistant, user, tool, and system message history.
- Internal DM threads already exist and are used for WELES governance interactions.
- Heartbeat and liveness infrastructure already detect broader forms of stalled work.
- Goal runs and tasks already carry status updates that can serve as evidence of progress or stagnation.

Current gap:

- there is no dedicated supervisor for turns that ended in a suspiciously unfinished state
- there is no daemon-owned WELES path that evaluates "was the last message actually done?"
- there is no recovery ladder for promise-like messages that are not followed by concrete action
- there is no persisted learning signal for "messages that often mean the agent stopped too early"

## Design Principles

- Active work is not stalled work.
- Unexpected end-of-turn supervision should happen after the normal stream/tool machinery has clearly stopped.
- A user reply inside the grace window should cancel automated recovery.
- The daemon should prefer soft recovery before stronger intervention.
- WELES should evaluate evidence, not just raw inactivity.
- Recovery decisions should produce durable traces so heuristics can improve over time.

## Healthy vs Unhealthy Waiting

The supervisor must not confuse normal waiting with a stalled turn.

### Healthy states

These should not trigger stalled-turn recovery:

- a tool call has been emitted and its result is still pending
- a managed command or external action is still running
- the send loop is still active, retrying, or waiting on an existing stream timeout path
- the user replied during the grace window
- the thread or goal run made concrete progress after the suspicious message

### Unhealthy states

These are the primary target:

- the stream ended unexpectedly without normal completion
- the last assistant message promised an action or signaled work-in-progress
- no concrete continuation followed within the grace window

Initial stalled-turn classes:

- `promise_without_action`
- `post_tool_result_no_follow_through`

## Detection Model

### Candidate creation

Create a stalled-turn candidate only after the daemon observes that a live turn has ended unexpectedly or a thread/goal has entered an equivalent suspicious post-turn state.

The grace timer starts when the daemon knows the turn is no longer actively progressing, not when text merely pauses.

The initial grace window is:

- `30s`

### Evidence inputs

For each candidate, gather:

- thread id
- optional goal run id
- task id when the turn belongs to a task
- last assistant message text
- whether the last assistant message followed a tool result
- whether the assistant text looks like a promise/progress message
- whether a new tool call followed
- whether a new assistant artifact-bearing message followed
- whether task or goal status advanced
- whether the user replied inside the grace window
- whether the thread still has an active stream cancellation entry or other live-runtime signal

### Promise-like message classification

The first version should use explicit heuristic classification before any learned model.

High-suspicion examples:

- "let me start drafting..."
- "working"
- "I'll do X now"
- "give me a moment"
- "I will produce the redesign now"

Signals that increase suspicion:

- duplicated progress filler
- present-tense commitment with no deliverable in the same message
- no tool calls after the message
- no follow-up assistant artifact
- the message immediately follows a tool result and should have produced synthesis or a next action

Signals that decrease suspicion:

- the message itself contains the requested deliverable
- a tool call followed shortly after
- the user replied
- task or goal state advanced

## Concrete Continuation Rules

A suspicious message is considered followed through when at least one of these happens within the active recovery window:

- the thread emits a tool call
- the assistant posts a substantive follow-up message that advances the promised work
- the user replies and the turn context legitimately changes
- the linked task or goal run advances state or progress

If none happen, WELES evaluates the turn as likely unfinished.

## WELES Evaluation

WELES should evaluate stalled-turn candidates rather than blindly continuing every silent thread.

### WELES decision contract

WELES returns:

- `done`
- `continue_required`
- `uncertain`

With:

- stall class
- confidence
- evidence summary
- recommended intervention text

### Decision behavior

- `done`: dismiss candidate and record why the message looked complete enough
- `continue_required`: inject a recovery nudge
- `uncertain`: default to a conservative soft continue on early attempts, then escalate only after repeated failure

The evaluation path should run over an internal WELES thread or a daemon-owned WELES supervision path, so this behavior is reusable and learnable.

## Recovery Ladder

The daemon should apply a bounded recovery ladder with increasing specificity.

### Attempt schedule

- attempt 1 after `30s`
- attempt 2 after `60s`
- attempt 3 after `120s`

### Attempt 1

Soft continue.

Example:

- "Continue from your last unfinished action."

### Attempt 2

Directed continue with evidence from the stalled turn.

Example:

- "You said you would draft the redesigned landing page, but no draft or tool/action followed. Continue with that work now."

### Attempt 3

Stronger directed continue with explicit recovery framing.

Example:

- "Your previous turn stopped after promising work but before taking action. Resume immediately from the unfinished step and produce the next concrete result."

## Strong Intervention

If all three retries fail, the daemon should move the thread or goal run into a stronger recovery state.

### Strong intervention behavior

- mark the thread/goal as `stuck_needs_recovery`
- emit an operator-visible stuck event
- persist the failed auto-recovery sequence
- allow a stronger in-thread recovery instruction, injected as a system-level recovery message

This stronger intervention is intentionally separate from the soft WELES nudges. It is the boundary where the daemon stops assuming this is a temporary lapse and starts treating it as a repeated execution failure.

## Internal Messaging Path

WELES recovery should reuse the daemon's internal messaging model rather than inventing a separate transport.

Recommended behavior:

- WELES writes recovery nudges as internal messages that target the responsible agent/thread context
- the nudge triggers the normal internal `continue` behavior so the thread resumes through the same message loop as ordinary work
- recovery messages remain attributable in history so later learning can correlate inputs with outcomes

The daemon should preserve a distinction between:

- soft WELES continuation nudges
- stronger system-level recovery intervention after bounded retries are exhausted

## Learning and Persistence

Every stalled-turn evaluation should create a persistent supervision record.

### Record contents

- thread id
- optional goal run id
- optional task id
- candidate created at
- evaluated at
- grace window / retry attempt
- last assistant message text
- preceding tool-result flag
- detected stall class
- WELES decision
- confidence
- intervention text
- whether recovery succeeded
- time to recovery or final escalation

### Purpose

This data supports future automation, including:

- ranking high-risk progress-message patterns
- learning which phrases correlate with unfinished turns
- measuring which nudge style works best
- distinguishing false positives from true stalled turns

## Integration Points

### Send loop

The send loop remains responsible for:

- active stream handling
- built-in retries
- tool execution flow
- final assistant persistence

It should only emit enough structured state for the stalled-turn supervisor to know whether a turn ended cleanly or suspiciously.

### Thread and task state

The stalled-turn supervisor should observe:

- thread message history
- active stream state
- task status
- goal run status

without taking ownership away from existing subsystems.

### Heartbeat and liveness

Heartbeat and generic liveness checks should remain coarse-grained.

The new stalled-turn supervisor is finer-grained and turn-specific. Heartbeat may later summarize its findings, but heartbeat should not be the primary runtime for `30s -> 60s -> 120s` recovery timing.

## Failure Modes and Safeguards

### False positive on healthy pause

Mitigation:

- do not trigger while tools or commands are still live
- cancel candidate when user replies
- require suspicious end-of-turn evidence, not raw inactivity alone

### Infinite auto-continue loop

Mitigation:

- bound retries to three attempts
- persist retry count with the candidate
- escalate after exhaustion instead of looping forever

### WELES nudges become noisy

Mitigation:

- prefer directed evidence-based nudges on later attempts
- record effectiveness so poor prompts can be improved

### Competing recovery systems

Mitigation:

- keep stream retry logic, stalled-turn recovery, and generic heartbeat/liveness checks as separate layers with explicit responsibilities

## Testing Strategy

Add daemon tests for:

- suspicious promise message with no follow-up becomes a candidate after `30s`
- tool execution still in progress does not create a candidate
- post-tool-result promise without action is classified as `post_tool_result_no_follow_through`
- user reply inside grace window cancels recovery
- attempt schedule uses `30s`, `60s`, `120s`
- WELES continue nudge is written through the internal messaging path
- successful continuation clears the candidate
- three failed retries escalate to `stuck_needs_recovery`
- supervision traces persist expected evidence for later learning

## Open Implementation Direction

Recommended implementation shape:

- add a dedicated daemon stalled-turn supervisor module
- store candidate and retry state in the daemon
- emit lightweight structured turn-end metadata from the send loop
- reuse WELES internal messaging for soft recovery
- add a stronger system-level intervention path for exhausted retries

This keeps the feature daemon-owned, cross-surface, and reusable for all threads and goal runs.
