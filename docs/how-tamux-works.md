# How tamux Works

This document describes the current system as it exists in the repository now: the daemon, UI clients, agent runtime, memory model, security/governance model, persistence model, and the self-orchestrating capabilities layered on top.

For implementation detail on the deeper agent internals, see [self-orchestrating-agent.md](./self-orchestrating-agent.md). For onboarding and local setup, see [getting-started.md](./getting-started.md). For the canonical memory architecture, see [tamux-memory.md](./tamux-memory.md). For the canonical security and governance model, see [tamux-security.md](./tamux-security.md).

## System Shape

tamux is a daemon-first terminal multiplexer with an agent runtime built into the daemon.

The important boundary is:

- The daemon is the source of truth.
- The TUI, Electron app, CLI, MCP server, and chat gateway are clients of that daemon.
- Agent threads, tasks, goal runs, approvals, transcripts, memory, and telemetry live with the daemon, not in the UI.

At a high level:

```text
Operator
  |
  +--> Electron / React UI
  +--> TUI
  +--> CLI
  +--> MCP clients
  +--> Slack / Discord / Telegram via gateway
           |
           v
      tamux daemon
           |
           +--> PTY session management
           +--> Agent runtime
           +--> Task queue
           +--> Goal runners
           +--> Approval flow
           +--> Persistence + telemetry
```

## Main Runtime Components

### Daemon

The daemon owns:

- PTY session lifecycle
- workspace and surface state
- agent threads and messages
- background task queue
- durable goal runs
- approvals
- transcript indexing
- operational and cognitive telemetry
- persistent memory files

Because the daemon owns all of this state, long-running work survives UI disconnects and can be reattached from another client.

### Clients

Each client is a different surface over the same daemon state:

- Electron is the richest desktop UI.
- The TUI gives an interactive terminal-native control plane.
- The CLI provides direct scripting and inspection.
- `tamux-mcp` exposes daemon capabilities to external MCP-capable agents.
- `tamux-gateway` lets chat platforms submit work into the daemon and receive results back.

### Session Manager

The session manager is the daemon subsystem responsible for terminal execution and terminal-adjacent persistence. It owns:

- PTY creation and teardown
- terminal input/output
- command history
- transcript index rows
- mission event rows
- snapshot metadata

Agent tools that act on terminals ultimately route through this layer.

## Agent Runtime

The built-in daemon agent is not just a chat wrapper. It is a stateful orchestrator with:

- threaded conversations
- tool execution
- background tasks
- goal planning and replanning
- sub-agent spawning
- memory maintenance
- health supervision
- learning/provenance features

The core hot path is:

1. A user or external client sends a message into a daemon thread.
2. The daemon persists that message immediately.
3. The daemon builds a prompt from:
   - configured system prompt
   - `SOUL.md`, `MEMORY.md`, `USER.md`
   - local skill index
   - operator-model summary when enabled
   - operational context summary
   - causal guidance
   - optional OneContext bootstrap recall
   - optional Honcho cross-session context when enabled
4. The agent calls tools in a bounded loop.
5. Tool results are written back into the thread and persisted.
6. The assistant response is persisted and broadcast to UI clients.
7. Background learning, provenance, and health state are updated around that turn.

## Conversation and Thread Model

Each daemon conversation is an agent thread with persisted messages.

Thread state includes:

- title
- ordered messages
- token counts
- upstream metadata for external-agent continuations
- timestamps

Messages include:

- role (`user`, `assistant`, `tool`, `system`)
- content
- tool metadata
- provider/model metadata
- token usage
- reasoning text when available

This thread state is written into SQLite and then hydrated back into memory on startup.

## Task Queue and Goal Runners

tamux separates short conversational turns from durable execution.

### Tasks

Tasks are daemon-owned units of work with fields for:

- status
- priority
- dependencies
- scheduling
- retry policy
- session affinity
- parent/child relationships
- approval waiting state

Tasks can be created by the operator, by the daemon agent, by goal runners, or by external clients.

### Goal Runs

Goal runs are the long-running autonomy layer.

A goal run typically does this:

1. Accept a high-level objective.
2. Ask the planning model for a step plan.
3. Convert steps into child tasks.
4. Dispatch those tasks through the daemon queue.
5. Monitor step/task outcomes.
6. Replan if needed.
7. Reflect on the finished run.
8. Optionally update memory and generate reusable skills.

Goal runs are durable. Their steps, events, status changes, and reflections are persisted.

## Tool Execution Model

The daemon agent uses a tool executor that exposes daemon-native capabilities and routes each tool to the correct subsystem.

Important tool families:

- terminal/session tools
- file and search tools
- history/recall tools
- task and goal tools
- memory tools
- skill tools
- semantic environment tools
- collaboration tools when enabled
- generated tool synthesis tools when enabled

Tool calls are not just prompt text. They become persisted tool messages and can also feed:

- execution traces
- causal traces
- provenance events
- operator feedback learning
- sub-agent health metrics

### Adversarial Self-Critique for Risky Actions

Before certain high-risk or suspicious tool executions, the daemon can run a local critique preflight.

That critique layer is a real subsystem, not just prompt style. It has explicit:

- advocate argument generation
- critic argument generation
- arbiter resolution
- persisted critique sessions, arguments, and resolutions
- operator-facing critique session retrieval through `get_critique_session`

At runtime, critique is used to:

- block or defer risky actions when the critic wins strongly
- convert a risky action into a safer variant through typed modifications/directives
- require explicit operator confirmation for selected high-impact actions
- learn from prior critique outcomes and recent causal traces

The current critique runtime is wired into risky tool execution such as:

- shell-style mutation tools
- file mutation tools
- explicit messaging tools
- delegated work (`spawn_subagent`, `enqueue_task`)
- high-impact guard-always actions like `switch_model`, `plugin_api_call`, and `synthesize_tool`

Critique payloads and operator-facing summaries are scrubbed for common secret patterns before persistence and retrieval so the critique evidence path does not echo raw tokens or similar sensitive strings back to the operator.

## Memory Model

tamux uses layered memory rather than treating chat history as the only context store.

At the minimum, that stack includes:

- `SOUL.md` for durable fire identity
- `MEMORY.md` for durable facts, conventions, and strategy hints
- `USER.md` for operator profile memory synchronized from daemon-owned profile state
- persisted threads, tasks, goals, checkpoints, and telemetry in SQLite
- recall systems over history and operational state
- procedural memory encoded as skills and skill variants
- provenance-backed durable memory facts
- semantic and structural memory adjacent to the workspace

The important boundary is that memory is daemon-owned, layered, and increasingly provenance-backed. Some of it is prompt-facing markdown. Some of it is structured state. Some of it is learned from execution and operator behavior. Some of it is still part of the forward architecture rather than a fully finished product surface.

For the full memory architecture, including episodic distillation, forge reflection, dream-state learning, semantic memory, and provenance-backed durable fact handling, see [tamux-memory.md](./tamux-memory.md).

## Persistence Model

tamux persistence is intentionally mixed: some data is best stored as structured SQLite rows, and some data is better kept as files.

### SQLite

SQLite is used for structured operational state such as:

- command history
- agent threads and messages
- tasks
- goal runs and events
- transcript index rows
- mission events
- operator profile fields, consents, interview/check-in events, and scheduled check-ins
- checkpoints
- provenance records
- collaboration sessions
- skill variant metadata
- memory provenance records

### Files

Files are used for:

- persistent markdown memory
- generated skills
- transcript bodies
- semantic telemetry JSONL ledgers
- WORM ledger files
- operator model sidecar state
- generated tool registry sidecar state

This split is deliberate. SQLite is the queryable truth for structured rows; file-based stores keep editable or append-only artifacts where that makes more sense.

## Telemetry, Provenance, and Auditability

The daemon records several types of telemetry:

- operational telemetry
- cognitive telemetry
- contextual telemetry
- provenance telemetry

The newer self-orchestrating capabilities also use:

- behavioral mission events in SQLite
- hash-linked WORM ledgers for provenance/integrity

This supports:

- replay
- auditing
- confidence/aging reports
- provenance exports
- explanation surfaces such as "why did the agent do that?"

## Self-Orchestrating Capabilities

The current codebase contains a set of optional higher-level capabilities layered over the baseline agent.

### M1: Operator Model

When enabled, the daemon learns aggregate patterns about the operator, such as:

- output density preference
- risk/approval behavior
- session rhythm
- attention behavior
- implicit feedback patterns

This is guarded by sub-controls so message statistics, approval learning, attention tracking, and implicit-feedback learning can be enabled independently.

### M2: Anticipatory Runtime

When enabled, the daemon can surface anticipatory items such as:

- morning brief style summaries
- predictive hydration of active threads
- stuck-work hints
- collaboration disagreement hints

### M3: Causal Traces

The daemon records causal explanations around decisions and outcomes so later tooling can answer:

- what happened
- why this path was chosen
- which option failed
- what the blast radius looks like

### M4: Genetic Skill Evolution

Generated and discovered skills now have variant metadata and lifecycle behavior:

- usage tracking
- success/failure settlement from real outcomes
- promotion/deprecation/archive
- automatic branching
- merge/convergence back into canonical variants

### M5: Semantic Environment Model

The daemon can inspect the local workspace and answer bounded semantic questions about:

- Rust crates and Node packages
- dependency and dependent relationships
- Compose service topology
- Terraform resources and modules
- Kubernetes resources and their local references
- import relationships
- learned conventions
- temporal history relevant to a target

### M6: Deep Storage / Provenance-backed Memory

Deep storage means durable memory facts are not just file diffs. They carry provenance, confidence state, and explicit operator-visible lifecycle.

That layer tracks things such as:

- where a durable memory fact came from
- when it was written
- which thread, task, or goal produced it
- whether it was confirmed or retracted
- which later operation explicitly invalidated it

See [tamux-memory.md](./tamux-memory.md) for the deeper memory architecture and operator-memory surfaces.

### M7: Collaboration Protocol

Sub-agents can coordinate through explicit collaboration sessions with:

- contributions
- disagreement records
- voting
- persisted shared state

### M8: Trusted Provenance

Goal/task/tool execution can emit provenance events into a signed or hash-linked audit trail so the system can later verify integrity and export evidence.

### M9: Implicit Feedback Learning

The daemon can learn from non-explicit operator behavior such as:

- fast denials
- correction-style messages
- tool fallback patterns
- attention transitions

### M10: Runtime Tool Synthesis

When enabled, the daemon can synthesize guarded tools from conservative CLI/OpenAPI surfaces, keep them in a registry, and promote proven tools into reusable assets.

## Safety Model

tamux is designed so autonomous execution passes through a real governance plane instead of relying on informal caution.

At a high level, the safety model includes:

- structured policy evaluation over transitions, not only raw commands
- scoped approvals with freshness and invalidation semantics
- risk and blast-radius analysis
- sandbox and isolation constraints
- adversarial critique for risky or suspicious actions
- provenance and audit requirements
- operator-visible blocked, approval-waiting, and review states

Autonomy is real, but it is not meant to be invisible, silent, or structurally under-governed.

For the full security and governance architecture, see [tamux-security.md](./tamux-security.md).

## Startup and Hydration

On daemon startup, the agent engine hydrates state from disk and SQLite, including:

- threads/messages
- tasks
- goal runs
- work context
- memory files
- operator model
- collaboration sessions

That means the runtime can resume from prior durable state instead of starting fresh on every process boot.

## Typical End-to-End Flow

A realistic long-running flow looks like this:

1. The operator opens the desktop UI or TUI and sends a goal.
2. The daemon persists the goal run and plans it.
3. Child tasks are created and dispatched.
4. The built-in agent executes turns, tools, and managed commands.
5. Risky work pauses in `awaiting_approval`.
6. Supervisors and anticipatory systems monitor progress.
7. Outcomes settle traces, skill usage, provenance, and memory candidates.
8. Reflection may write durable memory and generate or update skills.
9. The operator can inspect the run later from any client.

## Where to Read Next

- [getting-started.md](./getting-started.md): install and first-run path
- [self-orchestrating-agent.md](./self-orchestrating-agent.md): deeper execution internals
- [tamux-memory.md](./tamux-memory.md): canonical memory architecture
- [tamux-security.md](./tamux-security.md): canonical security and governance architecture
- [../skills/operating/memory.md](../skills/operating/memory.md): skill/operator guidance for curated memory usage
- [goal-runners.md](./goal-runners.md): goal-runner oriented behavior
