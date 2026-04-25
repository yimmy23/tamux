# Safety Architecture RFC

Status: proposed RFC for implementation planning
Last updated: 2026-04-09

This RFC defines the governance semantics for orchestrated execution in tamux.

For shared concepts and invariants, see [orchestration-safety-architecture.md](./orchestration-safety-architecture.md). For execution semantics, see [pty-fanout-orchestration-spec.md](./pty-fanout-orchestration-spec.md).

## Summary

The safety subsystem is responsible for deciding whether a run, stage transition, retry, rebind, or compensating action may proceed, under what constraints it may proceed, and what audit evidence must exist before and after execution.

This RFC makes three implementation-facing decisions explicit:

- safety decisions apply to **state transitions**, not just raw commands
- the output of safety evaluation is a **structured verdict**, not a boolean
- provenance quality is itself a **blocking dependency** for side-effecting work

The current harness MVP already uses that same governance model for its placeholder effect path, so effect dispatch and verification stay distinct even in the first landed slice. See [state-transition-harness.md](./state-transition-harness.md).

## Scope

This RFC owns:

- policy input schema
- governance verdict schema
- risk and blast-radius taxonomy
- approval requirements and approval invalidation rules
- sandbox and isolation requirements
- containment behavior
- rollback versus compensation obligations
- provenance and audit requirements
- break-glass rules
- re-evaluation triggers across the life of a run
- operator-facing safety expectations

This RFC does **not** own:

- PTY allocation or session transport
- lane dispatch mechanics
- frontend component layout
- exact CLI/TUI wording for every safety message
- memory-file editing behavior

## Normative Terms

The words **MUST**, **SHOULD**, and **MAY** are normative.

## Design Axioms

### 1. Safety governs transitions, not just commands

The governance subsystem evaluates transitions such as:

- run admission
- lane admission
- stage advancement
- retry or rebind
- compensation entry
- blocked-to-resumed
- final disposition

A shell command string MAY be one input into a decision, but it is never the only input.

### 2. Blast radius is compositional

Risk MUST be evaluated at multiple scopes:

- lane-local
- stage-wide
- run-wide

A locally narrow command MAY still be high-risk when executed concurrently across many targets or when applied after retries widened the affected scope.

### 3. Provenance is a safety dependency

A side-effecting transition MUST NOT advance if the system cannot prove enough about:

- who initiated the request
- what policy inputs were evaluated
- what verdict was returned
- what approval, if any, authorized the step
- what command or tool action was actually dispatched

### 4. Containment and compensation are first-class outcomes

The system MUST be able to express honest outcomes such as `Blocked`, `Compensated`, and `FailedWithPartialCompensation` instead of collapsing every unsafe or partially recoverable case into generic failure.

## Required Governance Interfaces

The implementation MAY choose concrete Rust types freely, but it MUST preserve the following observable fields and semantics.

### Policy input

Every governance evaluation MUST consume structured input containing at least:

- `run_id`
- `transition_kind` — for example `run_admission`, `stage_advance`, `lane_retry`, `resume_from_blocked`
- `stage_id` when the transition is stage-scoped
- `lane_ids` and `target_ids` in scope for the decision
- `requested_action_summary` — command, tool, or transition summary
- `intent_summary` — why the action exists
- `risk_dimensions`
  - destructiveness
  - scope
  - reversibility
  - privilege
  - externality
  - concurrency
- `blast_radius`
  - lane scope estimate
  - stage scope estimate
  - run scope estimate
- `environment_facts`
  - sandbox availability
  - network posture
  - filesystem scope
  - workspace or host type
  - privilege posture
- `approval_context`
  - prior approval IDs
  - approval freshness
  - whether conditions changed since approval
- `retry_or_rebind_history`
- `provenance_status`
- `rollback_or_compensation_hints`
- `initiator`
  - operator, agent, goal runner, or system path

Policy evaluation MUST operate on this structured context, not only a raw shell string.

### Governance verdict

Every governance decision MUST emit a structured verdict containing at least:

- `verdict_class`
  - `allow`
  - `allow_with_constraints`
  - `require_approval`
  - `defer`
  - `deny`
  - `halt_and_isolate`
  - `allow_only_with_compensation_plan`
- `risk_class`
  - `low`
  - `medium`
  - `high`
  - `critical`
- `rationale` — machine- and operator-readable reasons
- `constraints` — zero or more attached limits
- `approval_requirement` — absent unless approval is needed
- `containment_scope` — absent unless containment is required
- `compensation_requirement` — absent unless compensation planning is mandatory
- `freshness_window` or equivalent expiry semantics
- `policy_fingerprint` — a structural binding to the evaluated conditions

Binary allow/deny is insufficient for orchestration.

### Constraint vocabulary

The implementation MUST support attaching at least the following constraint types:

- sandbox required
- network denied
- network restricted
- filesystem scope narrowed
- target scope capped
- serial-only execution
- retries disabled
- retries require fresh checkpoint
- artifact retention elevated
- manual resume required after completion

If the implementation cannot honor an attached constraint, it MUST escalate the verdict rather than silently ignore the constraint.

## Risk Model

Risk SHOULD be derived from multiple dimensions rather than a single formula.

### Risk dimensions

The minimum dimension set is:

- **destructiveness** — delete, overwrite, terminate, or mutate potential
- **scope** — number and type of targets affected
- **reversibility** — feasibility of rollback or compensation
- **privilege** — elevated capability required or granted
- **externality** — whether effects escape the controlled local environment
- **concurrency** — whether parallel execution amplifies harm

### Risk classes

The resulting class MUST map to one of:

- **Low** — narrow scope, local impact, readily reversible
- **Medium** — bounded side effects or limited externality, still compensable
- **High** — broad, destructive, privileged, or externally visible actions
- **Critical** — major blast radius, poor reversibility, or severe trust consequences

The exact scoring function is an implementation detail, but for identical inputs it SHOULD produce stable output and stable rationale.

## Approval Semantics

### Approval matrix

The default policy shape is:

- **Low** — SHOULD auto-allow unless a higher-level policy says otherwise
- **Medium** — MAY auto-allow with constraints; SHOULD require approval when destructive, externally visible, or outside a trusted sandbox
- **High** — MUST require explicit operator approval, scoped constraints, and a clear containment path
- **Critical** — SHOULD default to deny; MAY proceed only via break-glass semantics

### Approval binding

Every approval record MUST bind to:

- run ID
- transition kind
- stage ID or transition ID when applicable
- target scope
- summarized risk rationale
- attached constraints
- freshness window or expiry
- policy fingerprint of the conditions being approved

If the target set, environment facts, command family, or privilege posture materially changes, an existing approval MUST NOT be silently reused.

### Approval invalidation triggers

Existing approval MUST be treated as stale when any of the following occur:

- target scope widens
- retry changes the effective blast radius
- a run moves from read-only to side-effecting behavior
- sandbox availability changes
- approvals expire
- the system loses required provenance continuity

## Sandboxing And Isolation

Governance MAY attach constraints to an `allow_with_constraints` verdict.

When a required sandbox or isolation boundary is unavailable, the verdict MUST escalate to one of:

- `defer`
- `require_approval`
- `deny`

The chosen escalation MUST match the effective risk class and rationale.

## Containment Model

Containment is a first-class governance outcome.

The system MUST support:

- **lane containment** — pause, isolate, or revoke capability for one lane
- **stage containment** — freeze new admissions or stage advancement
- **run containment** — halt future work while preserving forensic context

Containment is not equivalent to blind termination. The implementation SHOULD preserve enough context for diagnosis, operator review, and safe follow-up.

## Rollback And Compensation

The implementation MUST distinguish rollback from compensation.

- **Rollback** attempts to reverse changes and restore the prior state.
- **Compensation** applies forward actions that reduce, neutralize, or bound harm when rollback is impossible, unsafe, or incomplete.

Governance SHOULD require a compensation plan before execution for actions that are:

- destructive but not perfectly reversible
- externally visible
- concurrent across many targets
- likely to leave partially applied state

A run MAY legitimately finish as `Compensated` or `FailedWithPartialCompensation`.

## Provenance And Audit Requirements

For every material transition, the audit path MUST capture at least:

- initiator identity
- evaluated policy input summary
- returned verdict and attached constraints
- approval request and approval resolution
- command or tool invocation summary
- observed outputs and side effects
- intervention, containment, rollback, or compensation events
- final disposition

Execution provenance SHOULD be tamper-evident or hash-linked in a way that is compatible with tamux's broader WORM direction.

If provenance completeness is below the required threshold for a side-effecting step, governance SHOULD block advancement.

## Break-Glass Rules

Break-glass exists for exceptional cases where default policy would otherwise deny or indefinitely block action.

Break-glass execution MUST:

- require explicit operator action
- declare bounded scope
- carry a short validity window
- disable silent authorization reuse
- emit elevated audit and provenance detail
- record why ordinary policy was insufficient

Break-glass SHOULD be rare, reviewable, and visible in operator surfaces.

## Re-evaluation Triggers

Governance MUST re-evaluate when:

- the target set widens or materially changes
- retries change scope or environment assumptions
- a stage moves from read-only to side-effecting behavior
- approvals expire
- provenance continuity is incomplete
- containment changed the effective run context
- compensation planning became mandatory where it was previously optional

Safety decisions are therefore stateful over the life of a run rather than one-time preflight checks.

## Operator-Facing Requirements

Operator surfaces SHOULD make visible:

- why a transition was blocked, deferred, denied, or constrained
- which approval is being requested and for what exact scope
- whether containment happened at lane, stage, or run scope
- whether rollback or compensation is planned, in progress, or incomplete
- whether provenance quality itself is blocking execution

The UX SHOULD explain the governing reason, not merely show a generic denial.

## Suggested Implementation Slices

1. **Governance core**
   - define policy-input and verdict types
   - implement risk classification and verdict evaluation
   - surface structured rationale
2. **Approval binding**
   - persist approval records with expiry and policy fingerprints
   - reject stale approval reuse
3. **Containment and compensation**
   - support lane/stage/run containment states
   - represent compensation-required and compensated outcomes
4. **Audit closure and UX**
   - emit provenance records for every material transition
   - expose blocked/constrained reasons to operator surfaces

## Open Questions

The following questions should be resolved before implementation crosses subsystem boundaries:

1. **Policy ownership boundary**
   - Which policy inputs are computed inside `amux-daemon` versus supplied by higher-level callers?
2. **Risk scoring determinism**
   - Do we want a purely rule-based classifier first, or a weighted score with rule overrides?
3. **Approval persistence model**
   - Are approvals stored only in run-local state, or also in a longer-lived audit store for post-hoc review?
4. **Compensation authoring**
   - Is a compensation plan a structured object, a free-form artifact, or both?
5. **Break-glass visibility**
   - Which operator surfaces must display break-glass state synchronously: TUI, CLI, desktop, gateways?

## Acceptance Criteria

This RFC is implementation-ready when the resulting system can demonstrate all of the following:

- A side-effecting transition is evaluated from structured policy input rather than raw command text alone.
- Governance returns a structured verdict with rationale and optional constraints.
- Blast radius is computed at lane, stage, and run scope.
- High-risk transitions require explicit approval with bound scope and expiry.
- Existing approvals are rejected when conditions materially change.
- Required sandbox or isolation constraints are never silently dropped.
- The system can contain work at lane, stage, and run scope.
- The system distinguishes rollback from compensation in state and audit output.
- Provenance gaps can block advancement for side-effecting work.
- Break-glass execution is explicitly authorized, narrowly scoped, and audit-visible.
- Operator surfaces can distinguish `Blocked` from `Failed` and show the governing reason.
