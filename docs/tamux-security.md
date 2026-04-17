# tamux Security And Governance Architecture

tamux security is not a thin approval prompt in front of shell execution. It is a governance and trust architecture around transitions, side effects, provenance, isolation, critique, operator authority, and auditability.

The shortest correct description is:

- security governs transitions, not just commands
- risk is evaluated structurally, not only from raw text
- approvals are bound to scope and can go stale
- provenance is a safety dependency, not an optional log
- risky actions can be critiqued before they run
- containment, compensation, and break-glass are first-class outcomes

For the broader runtime architecture, see [how-tamux-works.md](./how-tamux-works.md). For the layered memory system that security protects and constrains, see [tamux-memory.md](./tamux-memory.md).

## What Security Means In tamux

In tamux, security is the combination of:

- governance
- approval semantics
- sandbox and isolation policy
- provenance integrity
- operator-visible risk handling
- secret scrubbing
- auditability
- critique for risky actions
- containment and compensation

This is broader than “don’t run dangerous shell commands.” The system is trying to make autonomy real without making it opaque or reckless.

## Security Is A Control Plane

The current architecture treats security as a control plane layered over execution rather than a bag of point checks.

Conceptually:

- the execution substrate reports what happened or what is about to happen
- the orchestration plane manages runs, stages, lanes, and advancement
- the governance plane decides whether advancement is allowed, constrained, deferred, denied, or isolated

This is the core idea behind:

- [orchestration-safety-architecture.md](./orchestration-safety-architecture.md)
- [safety-architecture-spec.md](./safety-architecture-spec.md)

If a system only looks at a shell string, it is too low-level to govern real autonomy. tamux is moving the decision point to run/stage/transition semantics.

## Security Starts Before Execution

tamux security is not only about what happened after the fact.

It begins before execution with:

- structured policy inputs
- risk classification
- sandbox availability checks
- approval requirements
- critique preflight for risky or suspicious actions
- provenance readiness checks

This means a risky action can be:

- allowed
- allowed only with constraints
- paused for approval
- deferred
- denied
- isolated
- forced to carry a compensation requirement

Binary allow/deny is not sufficient for the kind of orchestration tamux is designed to do.

## Governance Operates On Transitions

One of the most important ideas in tamux security is that governance applies to transitions, not only to raw commands.

Examples of governed transitions include:

- run admission
- lane admission
- stage advancement
- retry
- rebind
- blocked-to-resumed
- compensation entry
- final disposition

This matters because the same command can have very different blast radius depending on:

- how many targets are involved
- whether it is retried
- whether it is running in parallel
- whether the sandbox changed
- whether prior approvals are still valid

## Risk Is Structural, Not Cosmetic

The security model does not reduce risk to one keyword list.

Risk is derived from multiple dimensions such as:

- destructiveness
- scope
- reversibility
- privilege
- externality
- concurrency

That risk is also compositional:

- lane-local risk
- stage-wide risk
- run-wide blast radius

Something can be locally small but globally dangerous when fanned out across many targets or retried under widened scope.

## Structured Verdicts

The safety architecture defines structured verdicts rather than booleans.

Representative verdict classes include:

- `allow`
- `allow_with_constraints`
- `require_approval`
- `defer`
- `deny`
- `halt_and_isolate`
- `allow_only_with_compensation_plan`

This matters because real autonomy needs more than “yes” or “no.” It needs to express constrained permission, blocked-but-recoverable states, and compensation obligations honestly.

## Approvals Are Bound, Not Generic

tamux approvals are intended to be bound to real execution conditions rather than treated as permanent blanket permission.

An approval should bind to:

- run or transition scope
- target scope
- summarized risk rationale
- attached constraints
- expiry or freshness window
- policy fingerprint

This means an approval can become stale if:

- target scope widens
- privilege posture changes
- sandbox availability changes
- retries alter the effective blast radius
- provenance continuity is lost
- the system shifts from read-only to side-effecting work

This is a much stronger model than “the user clicked approve once.”

## Sandboxing And Isolation

Sandboxing is part of governance, not a separate afterthought.

The policy layer can require or constrain:

- sandbox use
- network denial or restriction
- narrowed filesystem scope
- serial-only execution
- restricted retries
- elevated artifact retention
- manual resume after completion

If a required constraint cannot be honored, the system should escalate rather than silently proceed as if nothing changed.

This is a key trust boundary: security claims must survive contact with actual runtime conditions.

## Critique As A Security Subsystem

tamux already ships a real adversarial self-critique layer for risky or suspicious tool execution.

That subsystem includes:

- explicit advocate, critic, and arbiter roles
- persisted critique sessions, arguments, and resolutions
- risk-triggered preflight before selected tools
- safer rewrites or modifications when critique finds a better path
- operator confirmation and continuation handling for high-impact paths
- operator-facing critique inspection
- secret-scrubbed critique evidence and summaries

This is not hypothetical. It is one of the clearest places where tamux security is already materially ahead of simplistic guardrails.

The roadmap source for this design is `.planning/iteration-2/13-adversarial-self-critique.md`, and the implementation crosswalk treats it as materially shipped.

## Provenance Is Part Of Security

In tamux, provenance is not merely for later analytics. It is part of the admissibility of risky work.

The safety RFC explicitly treats provenance quality as a blocking dependency for side-effecting transitions.

That means the system should be able to prove enough about:

- who initiated the action
- what policy inputs were evaluated
- what verdict was returned
- what approval authorized the step
- what tool or command actually ran

If provenance continuity is too weak, the system should not pretend that side effects were safely governed.

## Trusted Provenance And Auditability

tamux uses provenance in multiple ways:

- structured execution records in SQLite
- behavioral and causal telemetry
- memory provenance for durable facts
- hash-linked or WORM-oriented audit trails for stronger integrity

The architecture is intentionally trying to preserve:

- auditability
- replayability
- post-hoc review
- integrity evidence
- clear operator explanation surfaces

This is why “why did the agent do that?” is treated as a product question, not an afterthought.

## Secret Handling And Evidence Hygiene

A high-trust autonomous system cannot leak secrets through its own safety surfaces.

tamux already applies secret scrubbing to sensitive critique evidence and operator-facing summaries so that:

- critique logs do not become a credential exfiltration channel
- inspection tools can remain useful without replaying raw secrets
- audit evidence is safer to surface in operator UIs

This principle should be read broadly across the security model:

- safety evidence should be visible
- secrets should not be casually echoed
- operator trust depends on both transparency and restraint

## Containment, Compensation, And Break-Glass

tamux security is designed to express more realistic outcomes than “success” or “failure.”

Important outcomes include:

- blocked
- contained
- compensated
- partially compensated
- break-glass execution under elevated scrutiny

That matters in real operations because some unsafe or partially recoverable situations cannot be represented honestly by a generic failure badge.

The safety architecture is explicitly trying to preserve:

- lane containment
- stage containment
- run containment
- rollback where possible
- compensation where rollback is not enough
- explicit break-glass handling for exceptional cases

## Operator Authority Without Blind Micromanagement

tamux security is not trying to force the operator to click every button.

The design goal is:

- low-risk work can move
- medium-risk work can be constrained
- high-risk work requires explicit approval
- critical work trends toward denial or break-glass semantics

This is a practical middle ground between:

- powerless locked-down automation
- invisible agent autonomy with no trustworthy boundary

## Security And The Multi-Agent Runtime

Multi-agent systems create extra security demands because responsibility can move.

tamux therefore needs governance that survives:

- subagent delegation
- thread handoffs
- collaboration sessions
- goal-run orchestration
- retries and replans
- multi-target execution

This is why provenance and policy are tied to transitions and context, not only to whichever fire happened to emit the last natural-language message.

## Current Shipped Security Surface

The security and governance architecture in tamux already includes substantial real surfaces:

- approval workflows
- managed-command validation
- risk-aware gating
- adversarial self-critique for risky actions
- operator-model-aware risk and approval behavior
- provenance recording
- WORM/hash-linked integrity direction
- secret-scrubbed critique evidence
- blocked and approval-waiting execution states

This is already a richer architecture than the older “memory-and-security” framing suggested.

## Current RFC Direction

The dedicated orchestration and safety RFCs push the model further by making these ideas explicit:

- runs, stages, lanes, and targets are first-class governed objects
- policy checkpoints gate advancement
- approvals are structurally bound and invalidatable
- provenance quality can block side effects
- containment and compensation are first-class control-plane outcomes

Even where a specific UI or edge-case surface is still evolving, that is the correct architectural direction.

## Security Is Not Separate From UX

tamux security is operator-visible by design.

That includes:

- approval surfaces
- rationale and blast-radius summaries
- critique inspection
- audit/provenance review
- explicit blocked states

The point is not just to be safe internally. The point is to let the operator understand why the system did or did not proceed.

## Relationship To Memory

Security and memory are tightly coupled but distinct.

Memory answers:

- what should be remembered
- how it should be retrieved
- how durable facts evolve

Security answers:

- whether a transition may happen
- what constraints apply
- what evidence must exist
- how risky actions are challenged, approved, or contained

Memory provenance and execution provenance are related, but they are not the same thing.

See [tamux-memory.md](./tamux-memory.md) for the memory side of that split.

## The Right Mental Model

The correct mental model for tamux security is:

- governance over transitions
- structural risk evaluation
- scoped approvals
- real critique
- explicit constraints
- provenance as admissibility
- containment and compensation
- operator-visible trust boundaries

This is why tamux security should be described as a governance architecture for autonomous execution, not as a few shell prompts and warnings.

## Related Reading

- [how-tamux-works.md](./how-tamux-works.md)
- [tamux-memory.md](./tamux-memory.md)
- [orchestration-safety-architecture.md](./orchestration-safety-architecture.md)
- [safety-architecture-spec.md](./safety-architecture-spec.md)
- [self-orchestrating-agent.md](./self-orchestrating-agent.md)
