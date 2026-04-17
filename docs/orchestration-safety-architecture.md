# Orchestration And Safety Architecture RFC

Status: proposed parent RFC for implementation planning
Last updated: 2026-04-09
Original design session: 2026-04-08

This document defines the shared architecture for tamux orchestration and safety.

For the current daemon/runtime shape, see [how-tamux-works.md](./how-tamux-works.md). For the layered memory architecture, see [tamux-memory.md](./tamux-memory.md). For the current security and governance model, see [tamux-security.md](./tamux-security.md).

This parent RFC defines the shared model for two child RFCs:

- [PTY Fan-out Orchestration Spec](./pty-fanout-orchestration-spec.md)
- [Safety Architecture Spec](./safety-architecture-spec.md)

## Summary

tamux needs a control-plane architecture that can coordinate multi-target execution without collapsing safety into an afterthought.

This RFC establishes the shared model:

- runs coordinate work across targets through execution lanes
- stages define synchronization and advancement boundaries
- governance checkpoints constrain state transitions rather than merely annotating commands
- provenance binds execution facts and governance facts into one audit trail

The intent of this parent RFC is to freeze the shared nouns, invariants, and plane boundaries so the child RFCs can be implemented independently without semantic drift.

## Scope

This RFC owns:

- the conceptual system stack
- shared nouns and their boundaries
- control-plane authority and transition model
- run/stage/lane/target relationships
- cross-cutting invariants that both child RFCs must preserve
- scope boundaries between execution and governance responsibilities
- the minimum sequencing assumptions for implementation

This RFC does **not** own:

- detailed lane state-machine serialization
- detailed governance risk formulas
- frontend UX wording for every operator surface
- transport-level PTY protocol details
- memory-curation policy

Those belong to the child RFCs or adjacent product documents.

## Normative Terms

The words **MUST**, **SHOULD**, and **MAY** are normative.

## Why This Architecture Is Split

The system is intentionally split into one parent RFC and two child RFCs.

- This parent RFC owns shared nouns, invariants, and plane boundaries.
- The PTY fan-out RFC owns execution semantics: target binding, lane lifecycle, dispatch, cancellation, retries, aggregation, and operator-visible execution state.
- The safety RFC owns governance semantics: risk classes, approvals, isolation, compensation, break-glass, and provenance requirements.

The split exists to prevent execution semantics and governance semantics from drifting into one another while still forcing them to compose through the same control plane.

## System Stack

The design has three conceptual layers.

### 1. Execution substrate

This layer owns PTY lanes, terminal allocation, command streaming, process/session observation, and result capture.

It reports **facts** about what happened or is happening.

### 2. Orchestration control plane

This layer owns runs, targets, stages, sync modes, aggregation, retries, cancellation, and operator-visible run state.

It is the authority that decides whether execution facts and governance facts are sufficient for a run or stage to advance.

### 3. Safety and governance plane

This layer owns policy evaluation, blast-radius analysis, approvals, isolation, containment, compensation requirements, and provenance requirements.

It reports **permissions, constraints, and obligations**.

## Shared Object Model

### Run

A **run** is the top-level orchestration unit for one coordinated operation initiated by an operator, agent, goal runner, or higher-level tool.

A run owns at least:

- objective
- initiating context
- resolved target set
- lane set
- stage graph
- sync policy
- safety profile
- provenance root
- final summary

A run is the unit that may be paused, resumed, cancelled, compensated, reviewed, or audited.

### Target

A **target** is the thing the system intends to act on.

A target MAY resolve to:

- an existing terminal session
- a newly allocated PTY lane
- a directory or worktree
- a container
- a remote host
- a service endpoint
- a logical resource set

A target is not the same object as a lane. A target is the intended execution object; a lane is the execution vessel used to act on that target.

### Lane

A **lane** is a managed execution channel, typically backed by a PTY session.

A lane owns at least:

- lane ID
- terminal or session binding
- target binding
- current state
- command stream
- artifacts and outputs
- timing data
- intervention history

A lane MAY be ephemeral for a single run, reused within a run, or rebound across stages if policy allows.

### Stage

A **stage** is a synchronization boundary within a run.

Representative stages include:

- preflight
- distribute
- execute
- validate
- collect
- compensate
- finalize

A stage exists so orchestration semantics are explicit. Barrier, quorum, race, pipeline, serial, and best-effort behavior attach to stages rather than to vague run-wide parallelism.

### Policy checkpoint

A **policy checkpoint** is a required governance decision point before a stage or action may advance.

Examples include:

- blast radius escalates beyond the current authorization
- a destructive action is detected
- a required sandbox is unavailable
- compensation is mandatory but undefined
- operator approval is required before entry

Checkpoints are transition gates, not mere log annotations.

### Artifact

An **artifact** is any structured output produced by a lane or stage.

Examples include:

- stdout or stderr capture
- exit status
- generated file lists
- diff summaries
- validation reports
- rollback or compensation reports
- provenance bundles

Artifacts are inputs to aggregation, audit, memory curation, authorship summaries, and tool lineage.

## Core Invariants

Both child RFCs MUST preserve the following invariants.

### Invariant 1: Runs coordinate targets through lanes across stages

The architecture assumes a stable object relationship:

- a run contains stages
- stages admit lanes
- lanes are bound to targets
- targets define intended scope

No child implementation may collapse these objects into an indistinguishable execution blob at the observable API boundary.

### Invariant 2: Stage transitions are checkpointed

Every transition with meaningful execution or safety consequences MAY be constrained by governance checkpoints.

At minimum, the architecture MUST support checkpoints on transitions such as:

- `resolved -> preflighted`
- `preflighted -> executing`
- `stage_n -> stage_n+1`
- `executing -> compensated`
- `blocked -> resumed`

### Invariant 3: External side effects require both execution and governance truth

No state transition with external side effects is valid unless:

- the execution plane can prove what was dispatched or observed
- the governance plane can prove the action was admissible under current policy
- both truths are durably recorded in provenance

### Invariant 4: `Blocked` is not `Failed`

A run or stage MAY be structurally healthy yet unable to advance because policy refuses the next transition.

The architecture MUST preserve `Blocked` as a first-class observable state distinct from generic failure.

### Invariant 5: Aggregates outrank raw PTY text

Raw PTY output is evidence, not the control-plane truth.

Each stage MUST produce:

1. a lane result vector
2. a reduced stage aggregate

## Control-Plane Authority

The control plane joins execution facts and governance facts.

It owns authoritative state transitions for:

- run advancement
- stage advancement
- lane admission
- retries
- cancellation
- compensation
- final disposition

The execution substrate MAY report that work finished. The governance plane MAY report that work is admissible or blocked. The control plane decides whether that combination is sufficient for advancement.

## Canonical Sync Modes

The parent architecture standardizes the sync modes that the execution RFC must preserve semantically:

- **Barrier** — all participating lanes must finish before the stage advances.
- **Quorum** — the stage advances when a declared threshold is met.
- **Race** — the first acceptable result wins; remaining lanes may be cancelled or demoted.
- **Pipeline** — downstream work may start from admissible partial output before full global completion.
- **Serial** — exactly one participating lane advances at a time in declared order.
- **Best-effort fan-out** — all eligible lanes run, but partial failures do not automatically collapse the stage.

Child implementations MAY ship a subset first, but unsupported modes MUST fail explicitly rather than silently degrade.

## Intervention Scopes

The system MUST support intervention at three scopes:

- **Lane-scoped** — pause, kill, isolate, or rebind one lane.
- **Stage-scoped** — freeze entry or advancement for a stage.
- **Run-scoped** — halt, cancel, or compensate the entire run.

This scope model is shared across execution and governance even when the underlying mechanism differs.

## Scope Boundaries For Child RFCs

### PTY fan-out RFC must define

- exact lane state-machine semantics
- PTY or session allocation rules
- cancellation propagation
- stdout/stderr streaming behavior
- partial completion mechanics
- retry and rebind semantics
- aggregation payloads
- operator-facing orchestration UX expectations

### Safety RFC must define

- policy input schema
- governance verdict schema
- risk taxonomy
- approval matrix and invalidation rules
- sandbox and isolation rules
- compensation requirements by action class
- break-glass rules
- audit and provenance retention requirements

## Suggested Implementation Sequence

A sane implementation order is:

1. **Shared control-plane vocabulary**
   - define run, stage, lane, target, artifact, and checkpoint types
2. **Execution slice**
   - implement target resolution, lane lifecycle, dispatch tagging, and stage aggregates
3. **Governance slice**
   - implement policy inputs, verdicts, approvals, and containment
4. **Composition slice**
   - wire checkpoints into stage admission and advancement
5. **Operator-surface slice**
   - expose blocked/constrained state, provenance, retries, and compensation outcomes

This sequence is advisory, but any implementation MUST preserve the shared invariants above.

## Open Questions

The architecture core is stable, but these seams still need resolution before implementation crosses subsystem boundaries:

1. **Handoff-aware orchestration**
   - How do run ownership and provenance behave across agent-thread handoffs and subagents?
2. **Tool-call and MCP boundary**
   - Where does tool-protocol mediation stop and orchestration begin?
3. **Authorship and artifact provenance**
   - How are generated artifacts attributed when multiple agents and tools contribute to one run?
4. **Run-to-memory extraction**
   - Which run artifacts are eligible for episodic logging or memory-candidate extraction?
5. **Cross-surface UX consistency**
   - Which orchestration/safety states must be visible consistently across desktop, TUI, CLI, and gateway flows?

## Acceptance Criteria

This parent RFC is implementation-ready when the resulting architecture can demonstrate all of the following:

- The system exposes stable run, target, lane, stage, checkpoint, and artifact concepts at the control-plane boundary.
- The control plane, execution plane, and governance plane have non-overlapping responsibilities.
- Stage advancement is described as a checkpointed control-plane transition rather than an execution-only event.
- `Blocked` is represented distinctly from `Failed`.
- Every stage produces both per-lane and reduced aggregate truth.
- The child RFCs can evolve independently without contradicting the shared invariants in this document.
