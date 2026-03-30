# Constraint-State Upgrade Design

**Goal**: Evolve existing negative knowledge into a richer constraint-state system with simple propagation, while preserving current episodic memory and prompt-injection architecture.

## Context

The repository already implements negative knowledge and counter-who primitives in `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs` and `crates/amux-daemon/src/agent/episodic/counter_who.rs`. The gap is not absence of constraint tracking, but that constraints are currently flat records. They do not express lifecycle state such as suspicious vs dead, do not record why a state was inferred versus directly observed, and do not propagate weakly to related approaches.

This design upgrades the existing `NegativeConstraint` model in place. It avoids building a separate graph engine and instead introduces explicit state, lightweight relationship metadata, and deterministic one-hop propagation rules.

## Scope

In scope:
- Add explicit constraint state to negative knowledge
- Persist state and propagation metadata in SQLite
- Upgrade formatting so prompt injection distinguishes soft and hard constraints
- Add simple propagation across related constraints based on existing fields
- Keep backward compatibility for existing rows and call sites

Out of scope:
- Recursive graph search or general-purpose constraint solver
- Full runtime failure taxonomy
- Self-assessment to mode-shift wiring changes
- New UI for inspecting constraints

## Approach Options

### Option 1: Evolve `NegativeConstraint` in place (recommended)

Add state and propagation metadata directly to the current episodic model, then upgrade existing query, formatting, and persistence paths.

Pros:
- Reuses existing storage, cache, prompt injection, and consolidation hooks
- Smallest migration surface
- Lowest risk of architectural duplication

Cons:
- Less theoretically pure than a separate graph engine
- Propagation rules must stay intentionally simple

### Option 2: Add a separate constraint graph module

Create a new graph-specific data model and derive prompt-facing constraints from it.

Pros:
- Cleaner conceptual separation
- More room for future graph algorithms

Cons:
- Duplicates existing negative knowledge path
- Requires broader migration and integration work now
- Higher delivery risk

## Recommended Design

### Data Model

Extend `NegativeConstraint` with:
- `state: ConstraintState`
- `evidence_count: u32`
- `direct_observation: bool`
- `derived_from_constraint_ids: Vec<String>`
- `related_subject_tokens: Vec<String>`

Add `ConstraintState` as a compact enum:
- `Suspicious` — weak signal; avoid repeating blindly but not a hard ban
- `Dying` — repeated evidence indicates this path is likely poor
- `Dead` — strong evidence; treat as ruled out unless conditions change

State meaning is behavioral, not emotional. It lets the agent distinguish tentative caution from a firm prohibition.

### State Transitions

Constraints move monotonically upward unless they expire:
- new inferred constraints start at `Suspicious`
- repeated evidence upgrades to `Dying`
- strong direct evidence or repeated failure upgrades to `Dead`

Initial rules:
- direct failure-derived constraints created from failed episodes start at `Dying`
- a direct constraint with high confidence (`>= 0.85`) starts at `Dead`
- repeated matching evidence increments `evidence_count`
- `evidence_count >= 2` upgrades `Suspicious -> Dying`
- `evidence_count >= 3` or direct high-confidence evidence upgrades to `Dead`

No downgrade path is added in this pass. Expiry remains the reset mechanism.

### Simple Propagation

Propagation is shallow and deterministic.

When a constraint becomes `Dead`, find active related constraints using this exact rule:
- related if `solution_class` is present on both constraints and equal, and the two constraints share at least 2 normalized subject tokens, or
- if `solution_class` is absent on either side, related only when the two constraints share at least 3 normalized subject tokens

For each related active constraint:
- if currently `Suspicious`, upgrade to `Dying`
- if already `Dying`, keep `Dying` unless direct evidence independently promotes it
- append the source constraint id to `derived_from_constraint_ids`
- only set `direct_observation = false` when the target constraint has never had direct evidence; never overwrite `true` to `false`

Propagation limits:
- one hop only
- max 10 related constraints per propagation event
- never create recursive propagation loops
- never lower a state

This produces useful behavior without introducing an opaque graph engine.

### Prompt Injection

Update formatting in `format_negative_constraints` so constraints are grouped by state and phrased according to strength:
- `Dead`: `DO NOT attempt`
- `Dying`: `Avoid unless you have new evidence`
- `Suspicious`: `Use caution`

Each rendered constraint should include:
- state
- reason
- confidence
- whether it is direct or inferred
- optional provenance summary when derived from another constraint

Prompt order should be strongest first: `Dead`, then `Dying`, then `Suspicious`.

### Persistence and Migration

Add SQLite columns to `negative_knowledge`:
- `state TEXT NOT NULL DEFAULT 'dying'`
- `evidence_count INTEGER NOT NULL DEFAULT 1`
- `direct_observation INTEGER NOT NULL DEFAULT 1`
- `derived_from_constraint_ids TEXT NOT NULL DEFAULT '[]'`
- `related_subject_tokens TEXT NOT NULL DEFAULT '[]'`

Migration behavior:
- existing rows default to `Dying`
- existing rows default to `evidence_count = 1`
- existing rows default to `direct_observation = 1`

This preserves current semantics while enabling richer behavior.

### Runtime Integration

Upgrade these paths:
- failed-episode recording computes initial state and subject tokens
- add/update constraint path merges matching constraints instead of always inserting a fresh independent record when the subject and solution class match closely enough
- propagation runs after a constraint is created or upgraded to `Dead`
- cache refresh and expiry continue to work with the richer model

Matching heuristic for merge/reuse:
- same `solution_class` and exact same normalized `subject`, or
- exact same normalized `subject` when `solution_class` is absent

Normalization in v1 is explicit: lowercase, split to alphanumeric tokens, drop tokens shorter than 3 chars, sort, and dedupe.

## Files Likely To Change

- `crates/amux-daemon/src/agent/episodic/mod.rs`
- `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- `crates/amux-daemon/src/agent/episodic/schema.rs`
- `crates/amux-daemon/src/agent/system_prompt.rs` if formatting integration needs adjustment
- targeted tests in the same modules

## Testing Strategy

Use TDD.

Add tests for:
- state formatting and ordering in prompt text
- token normalization and relation detection
- state upgrade thresholds
- propagation from `Dead` to related constraints
- SQLite row round-trip with new columns
- backward-compatible defaults for old-style rows

## Risks and Mitigations

- Over-propagation causing bad advice
  - Mitigate with one-hop rules, small caps, and conservative matching

- Existing data compatibility issues
  - Mitigate with additive schema migration and defaults

- Prompt bloat
  - Mitigate by preserving current caps and sorting strongest constraints first

## Success Criteria

This upgrade is successful when:
- negative knowledge distinguishes `Suspicious`, `Dying`, and `Dead`
- repeated evidence upgrades constraint state deterministically
- a `Dead` constraint can weaken nearby related paths through simple propagation
- prompt injection surfaces the richer state clearly
- existing episodic behavior keeps working without destructive migration
