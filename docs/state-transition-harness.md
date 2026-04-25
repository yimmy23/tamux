# State-Transition Harness (current landed slice)

This document tracks the additive harness subsystem that is now real in `crates/amux-daemon/src/agent/harness/`.

## What exists now

The daemon now contains a persisted substrate for these harness primitives:

- observations
- beliefs
- goals
- world-state snapshots
- tensions
- commitments
- effects
- verification records
- procedures
- action contracts

Persistence is split into two layers:

1. **`harness_state_records`** in SQLite as an append-only envelope log for the harness entities,
2. **projection code** in `agent/harness/runtime.rs` that rebuilds the current state from those persisted envelopes.

That gives the subsystem an auditable persisted trail without rewriting the rest of the daemon.

## Minimal closed loop now implemented

The current loop now does more than the original placeholder path. It can:

1. read persisted harness state,
2. ingest an observation,
3. derive both a working belief and a world-model belief,
4. project a first real world-state snapshot,
5. detect persisted tensions for contradiction, information gap, risk escalation, stale commitment, drift, and opportunity,
6. select the highest-priority tension set,
7. build a governed effect contract that explicitly declares execution kind, risk hint, blast-radius hint, preconditions, expected effects, and named verification gates,
8. run role-structured commitment selection with interpreter/cartographer, strategist, executor, verifier, and skeptic assessments,
9. persist a distinct effect record,
10. evaluate explicit verification gates separately from dispatch success,
11. settle commitment state based on verification outcome,
12. emit event-log plus provenance trace entries.

## Governed effect contracts and verification gates

Effect contracts are now explicit first-class records.

Each contract carries:

- `execution_kind` (`read_only` vs `mutating`)
- reversibility
- risk and blast-radius hints
- preconditions
- expected effects
- verification strategy text
- a list of named verification gates
- the governance input that produced the transition verdict

The first verification gates are:

- governance verdict
- persisted effect row
- effect output evidence
- desired-state match when a desired state is declared

This keeps verification distinct from effect dispatch and makes governance part of the contract rather than an implicit side note.

## World model and tension engine

The current world model is intentionally small but real.

A `WorldStateRecord` persists:

- observed state
- desired state (optional)
- contradictions
- unknowns
- risk flags
- opportunities
- stale commitment references
- active tension ids
- next-step hint

Tensions are now driven from that world state and persisted as reconstructable records. The current detector emits these kinds:

- contradiction
- information gap
- risk escalation
- stale commitment
- drift from desired state
- opportunity

Next-step selection is therefore driven by persisted tensions rather than only by the incoming chat turn.

## Role-structured commitment critique

Commitments now include role assessments from a shared-state internal structure:

- interpreter/cartographer
- deliberator/strategist
- executor
- verifier/auditor
- skeptic/critic

The chosen commitment persists:

- rationale
- linked world state and source tension
- expected effects
- verification plan
- critique summary
- per-role assessments

That means candidate execution is critiqued before the effect path runs, even though this slice still uses one shared runtime rather than separate role threads.

## Procedure learning and visibility now shipped

The additive harness now includes one real procedure-distillation loop and one real developer visibility surface.

### Procedure distillation

Procedure records are no longer just one-off placeholders.

Each persisted procedure now carries:

- a `trace_signature` that summarizes the repeated governed trajectory
- applicability conditions derived from effect mode, tension kind, and goal context
- whether the latest outcome was verified
- cumulative successful and failed trace counts for that signature
- confidence derived from repeated verified success versus failed outcomes
- a preferred effect ordering that can push the later successful mode earlier in future fallback selection
- linkage back to the source verification record and supporting details

When the same governed trace succeeds repeatedly, the harness promotes the newest procedure record from `candidate` to `learned`. Failed outcomes do not disappear; they reduce confidence and keep the failure path visible in the persisted procedure metadata.

This is still intentionally bounded learning:

- the loop distills repeated harness-local traces, not arbitrary daemon behavior
- confidence is heuristic and auditable, not opaque model state
- learned procedures are persisted harness records, not autonomous self-modifying code

### Visibility surface

The daemon now exposes a developer-facing `show_harness_state` tool.

That surface projects persisted harness state for a chosen thread, goal run, or task scope and returns structured sections for:

- latest world state
- recent beliefs
- active and recent tensions
- latest and recent commitments
- latest and recent effects
- latest and recent verification results
- latest, recent, and learned procedures
- the latest effect contract

This makes the distinction between belief, chosen action, dispatch result, and verified outcome inspectable without reading raw logs only.

## What this slice still does **not** claim

This landed slice is still intentionally bounded.

It does **not** yet provide:

- a full TUI or desktop UI panel for harness state
- broad integration into the main daemon agent loop
- multi-step learned policy distillation beyond repeated local trace signatures
- automatic execution from learned procedures without fresh governed evaluation

Those remain for later phases.

## Verification semantics

Verification remains first-class and separate from effect dispatch success.

The harness now persists:

- an effect record that captures dispatch outcome and governance verdict,
- a verification record that evaluates explicit gate results,
- a commitment state transition that depends on verification rather than only tool success.

That distinction is the contract for later effectors and operator-visible surfaces.
