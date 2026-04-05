# Agentic Mission Control

## Core User Journey

1. Session initialization
   - User opens a workspace and a daemon-backed PTY attaches to the active pane.
   - MEMORY and USER context are hydrated into the mission panel.
2. Goal creation
   - Human starts a durable goal run from Mission Control instead of issuing only a single prompt.
   - The daemon persists the goal immediately so it can survive UI disconnects and restarts.
3. Planning and child-task delegation
   - The built-in daemon agent generates a structured plan.
   - Executable steps are translated into child tasks on the daemon queue.
4. Agent reasoning and execution
   - INNER_MONOLOGUE and SCRATCHPAD payloads stream into the Reasoning Trace plane.
   - Operational, cognitive, and contextual telemetry are persisted alongside goal-run state.
5. HITL approval
   - High-risk managed commands pause behind the structured Security Interceptor modal.
   - Approval decisions flow back to the daemon, which resumes the waiting child task and goal run.
6. Reflection and durable output
   - On success, the daemon records a final reflection, may append a durable memory update, and may generate a reusable skill from the trajectory.

## Low-Fidelity Wireframes

These wireframes describe the current mission-control layout direction and interaction model. They are intentionally approximate, not a pixel-perfect screenshot of every shipped panel state.

### Mission Control

```text
+----------------------------------------------------------------------------------+
| Title / Status / Workspace Context                                               |
+-------------------------+--------------------------------------+----------------+
| Sidebar / Surfaces      | Shared REPL / Terminal Pane         | Mission Panel  |
|                         |                                      |                |
|                         |  [Shared Cursor Badge]              | Threads        |
|                         |                                      | Trace          |
|                         |  live output / managed execution     | Goal Runners   |
|                         |                                      | Tasks / Trace  |
|                         |                                      | Context / Graph|
+-------------------------+--------------------------------------+----------------+
| Status Bar: daemon | mission | trace | ops | logs | vault | search | settings   |
+----------------------------------------------------------------------------------+
```

### Approval Interceptor

```text
+------------------------------------------------------------------+
| Approval Required                              [critical/high]    |
| High-impact shell command intercepted                            |
|                                                                  |
| Command                                                          |
|   rm -rf ...                                                     |
|                                                                  |
| Blast Radius | Scope                                             |
|                                                                  |
| Reasons: destructive recursive delete | network access requested |
|                                                                  |
|                           [Deny] [Allow Once] [Allow For Session]|
+------------------------------------------------------------------+
```

### Time Travel Vault

```text
+----------------------------------------------------------------------------+
| Search / Filters / Timeline Mode                                           |
| Timeline Cards                                                             |
| Scrubber ----------------------------------------------------------------- |
| Target: checkpoint or command                                              |
|                                                                            |
| Left: transcript list or timeline rows  | Right: immutable StaticLog view  |
+----------------------------------------------------------------------------+
```

Current desktop Session Vault behavior also includes a memory provenance mode for durable facts. That view shows active/uncertain/confirmed/retracted states, supports direct confirm and retract actions, and renders persisted `retracts` relationships between memory entries. The TUI does not yet expose the same direct controls.

## Goal Runner Model

- A **goal run** is the durable unit of autonomy.
- A **child task** is the executable unit that runs on the daemon queue.
- Goal runs own plan generation, replanning, reflection, and durable outputs.
- Child tasks own queueing, lane scheduling, approval waits, retries, and managed command execution.

Current top-level goal-run states:

- `queued`
- `planning`
- `running`
- `awaiting_approval`
- `paused`
- `completed`
- `failed`
- `cancelled`

## Color Taxonomy

- Human input: `var(--success)`
- Agent-managed execution: `var(--accent)`
- Goal-run planning and orchestration: `var(--mission)`
- Reasoning trace: cool blue text on translucent navy surfaces
- Security warnings: `var(--warning)`
- Critical danger: `var(--danger)`
- Background hierarchy:
  - canvas: `--bg-primary`
  - panels: `--bg-secondary`, `--bg-surface`
  - overlays: translucent glass with `--glass-border`

## Z-Axis Depth Model

- Z0: terminal canvas and static content
- Z1: split-pane chrome, tabs, context HUD, shared cursor badge
- Z2: mission side panel and graph surfaces
- Z3: modal overlays including the Security Interceptor

## BFO Guidance

- Live terminal remains xterm-backed and mutable.
- Historical content is cast into StaticLog to avoid deep re-render churn.
- Reasoning trace is chunked into immutable cards rather than a single giant text node.
- Session Vault timeline limits event density and exposes a scrubber rather than replaying the full corpus.

## Shared Cursor Rules

- Human cursor mode is shown for direct keyboard input in the REPL.
- Agent cursor mode is shown while daemon-managed commands are starting or running.
- Mission mode should be visually distinct when a long-running goal is planning or coordinating child work.
- Approval mode is shown while a structured approval request is pending.
- Idle mode is shown when the lane is quiescent.
