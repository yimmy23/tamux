# PTY Fan-out Orchestration RFC

Status: proposed RFC for implementation planning
Last updated: 2026-04-09

This RFC defines execution semantics for orchestrated work across one or more PTY-backed lanes in tamux.

For shared concepts and invariants, see [orchestration-safety-architecture.md](./orchestration-safety-architecture.md). For governance semantics, see [safety-architecture-spec.md](./safety-architecture-spec.md).

## Summary

The orchestration subsystem turns an operator or agent request into a run with:

- resolved targets
- execution lanes
- explicit stages
- synchronization semantics
- retries and cancellation behavior
- structured aggregates for the control plane

This RFC makes the execution contract implementation-ready by specifying required state, stage, and aggregation semantics independently from the safety plane.

## Scope

This RFC owns:

- target resolution into execution lanes
- lane lifecycle and rebinding semantics
- stage membership and sync behavior
- command dispatch and output streaming
- cancellation and retry propagation
- partial failure handling
- aggregation outputs for the control plane
- operator-visible execution behavior

This RFC does **not** own:

- risk classification
- approval policy
- compensation requirements by action class
- break-glass rules
- long-term memory curation policy

Those belong to the safety architecture or higher-level product documents.

## Normative Terms

The words **MUST**, **SHOULD**, and **MAY** are normative.

## Design Axioms

### 1. The control plane is the authority for advancement

The execution substrate may know that a lane is technically runnable, but a lane MUST NOT begin stage work until the control plane admits it.

### 2. Target and lane are distinct objects

A target is the thing the system intends to act on. A lane is the execution vessel used to act on that target. Multiple stages MAY reuse a lane for one target, and one run MAY rebind a target to a replacement lane after contamination or failure.

### 3. Structured aggregates outrank raw PTY text

Raw stdout and stderr are evidence, not the control-plane truth. Stage aggregates and per-lane result vectors are the authoritative execution outputs consumed by orchestration and safety.

## Required Execution Interfaces

Concrete Rust shapes are an implementation detail, but the following observable contracts MUST exist.

### Run plan

A run plan MUST include at least:

- `run_id`
- `objective`
- `initiator`
- `targets`
- `candidate_lanes`
- `stage_graph`
- `sync_policies`
- `provenance_root`

### Target resolution record

Every resolved target MUST produce a record containing at least:

- `target_id`
- `target_kind`
- `requested_scope`
- `resolved_binding`
- `binding_origin` — reused vs newly allocated
- `constraints_inherited_from_target_or_environment`

A reused session MUST be provenance-visible so later audits can distinguish reuse from fresh isolation.

### Stage contract

Every executable stage MUST declare:

- `stage_id`
- `participating_lanes`
- `input_artifacts`
- `sync_mode`
- `entry_criteria`
- `success_criteria`
- `per_lane_timeout_policy`
- `cancellation_policy`
- `expected_output_contract`

### Dispatch record

Every dispatched command MUST be tagged with at least:

- `run_id`
- `stage_id`
- `lane_id`
- `attempt_index`
- `target_id`
- `dispatch_timestamp`

### Lane result

At stage close, each participating lane MUST produce a result containing at least:

- `lane_id`
- `target_id`
- `final_lane_state`
- `attempt_index`
- `exit_condition`
- `artifact_references`
- `intervention_events`
- `timing_summary`

### Stage aggregate

At stage close, the execution subsystem MUST emit one reduced record containing at least:

- `stage_id`
- `sync_mode`
- `aggregate_outcome`
- `winning_or_threshold_rationale`
- `counts_by_lane_outcome`
- `deviations_worth_attention`
- `recommendation_for_next_stage_eligibility`

## Run Lifecycle

A run SHOULD move through the following observable lifecycle:

1. **Declared** — intent exists, but targets or lanes may still be unresolved.
2. **Resolved** — targets, candidate lanes, and stage graph are known.
3. **Preflighted** — feasibility and governance checks for stage entry have cleared.
4. **Executing** — one or more stages are running.
5. **Collecting** — outputs are being reduced into lane vectors and stage aggregates.
6. **Finalizing** — cleanup, summary, and closure are in progress.

Terminal outcomes MUST preserve at least:

- `Completed`
- `Failed`
- `Cancelled`
- `Blocked`
- `Compensated`

`Blocked` MUST remain distinct from `Failed`.

## Target Resolution

Target resolution MUST complete before a run enters `Preflighted`.

A target MAY resolve to:

- an explicitly requested existing terminal session
- a compatible idle session selected for reuse
- a newly allocated managed terminal lane
- a non-PTY logical target that still requires a coordinating lane record

Target resolution MUST fail explicitly rather than silently degrading into a different target class.

## Lane Lifecycle And Transition Rules

A lane SHOULD move through the following lifecycle:

1. **Planned**
2. **Binding**
3. **Allocated**
4. **Ready**
5. **Admitted**
6. **Running**
7. **Draining**
8. **Completed / Failed / Cancelled**

Exceptional states MAY include:

- **Isolated**
- **Rebinding**
- **Dropped**

The serialized state machine MAY vary internally, but the observable semantics above MUST be preserved.

### Admission rules

A lane MUST NOT start stage work until all of the following are true:

- target binding is complete
- stage entry criteria are satisfied
- the governance checkpoint returned an admissible verdict
- required operator approval, if any, is recorded

### Rebinding rules

A lane MAY be rebound only when:

- the prior lane is no longer trusted, available, or compatible
- a new binding is recorded in provenance
- retry scope does not silently widen the target set

## Stage Sync Modes

The implementation MUST preserve the following sync semantics.

### Barrier

The stage completes only when all admitted lanes reach an acceptable terminal result or an unrecoverable condition aborts the stage.

### Quorum

The stage completes when the declared threshold is met. Non-winning lanes MAY continue, drain, or be cancelled according to stage policy.

### Race

The first admissible result wins. Losing lanes SHOULD receive cooperative cancellation first and MAY be force-terminated after a bounded grace period.

### Pipeline

Admissible partial artifacts MAY unlock downstream work before all participating lanes complete. The control plane MUST preserve causality between emitted artifacts and downstream consumers.

### Serial

Exactly one lane is active at a time in declared order. A later lane MUST NOT begin until the current lane reaches a handoff point allowed by stage policy.

### Best-effort fan-out

All eligible lanes run. Partial failure is recorded in the stage aggregate but does not automatically fail the stage unless the stage policy defines a minimum success floor.

If the first implementation ships a subset of sync modes, unsupported modes MUST fail explicitly at planning or admission time rather than silently degrading.

## Dispatch And Streaming

The execution subsystem MUST capture:

- stdout stream
- stderr stream
- exit status or signal
- start and end timestamps
- dispatch metadata
- detected terminal health anomalies

Streaming output SHOULD be available live to operator surfaces, but structured lane and stage aggregates remain authoritative.

## Cancellation Propagation

Cancellation MAY be initiated at lane, stage, or run scope.

- **Lane cancellation** affects one lane and SHOULD preserve the rest of the stage when policy allows.
- **Stage cancellation** stops new admissions to the current stage and resolves active lanes according to stage cancel policy.
- **Run cancellation** prevents future stages and initiates finalization or compensation handling.

Cooperative cancellation SHOULD be attempted before forceful termination unless containment policy requires immediate isolation.

## Retry And Rebind Semantics

Retries MUST be explicit and provenance-visible.

Each retry MUST record:

- retry reason
- previous attempt index
- whether the lane is reused or rebound
- any widened or narrowed scope
- new checkpoint verdicts when conditions changed

A retry MAY reuse the same lane only when the lane is still considered clean and the target binding remains valid. Otherwise the system SHOULD allocate a replacement lane.

Retries MUST NOT silently widen the target set.

## Partial Failure Handling

The execution subsystem MUST preserve enough structure for the control plane to distinguish:

- total run failure
- lane-local failure with recoverable stage continuation
- stage failure with preserved run integrity
- race loser cancellation
- quorum miss
- operator cancellation
- governance-driven isolation

A single bad lane MUST NOT automatically collapse a multi-lane run unless stage policy or governance requires that outcome.

## PTY And Session Reuse Rules

Reusing an existing terminal session SHOULD be allowed only when all of the following hold:

- the operator explicitly requested reuse, or orchestration policy allows reuse for the run class
- the session is live and not occupied by incompatible work
- environment assumptions required by the stage remain valid
- provenance records the reuse decision

Otherwise the system SHOULD allocate a dedicated managed lane.

## Operator-Facing Requirements

The operator surface SHOULD expose:

- run status and current stage
- per-lane state and target mapping
- live output with provenance labels
- blocked or approval-pending transitions
- retries, rebinds, and isolation events
- stage aggregate summaries rather than only raw terminal transcripts

Operator-visible status MUST distinguish `Blocked` from `Failed`.

## Suggested Implementation Slices

1. **Target resolution and lane state**
   - implement run plan, target resolution records, and lane lifecycle state
2. **Stage execution core**
   - implement admission rules, sync mode handling, and dispatch tagging
3. **Retries, cancellation, and rebinding**
   - implement explicit retry records and scoped cancellation propagation
4. **Aggregation and operator surfaces**
   - emit lane result vectors and stage aggregates for UI and audit consumers

## Open Questions

The following design seams should be resolved before implementation crosses subsystem boundaries:

1. **Initial sync-mode subset**
   - Which modes ship in the first daemon slice: barrier only, barrier plus race, or the full canonical set?
2. **Non-PTY targets**
   - Do non-PTY logical targets share the same lane abstraction fully, or only enough to preserve provenance and control-plane uniformity?
3. **Terminal contamination detection**
   - What exact signals mark a lane as contaminated and require rebinding instead of retry-in-place?
4. **Streaming retention policy**
   - How much raw PTY output is kept inline versus offloaded into referenced artifacts?
5. **Pipeline mode gating**
   - Do downstream stages subscribe to partial artifacts directly, or only through control-plane aggregate approval?

## Acceptance Criteria

This RFC is implementation-ready when the resulting system can demonstrate all of the following:

- Targets resolve into explicit lane bindings before execution begins.
- Reused versus newly allocated bindings are recorded in provenance.
- Lanes do not begin work before control-plane admission.
- Stages declare membership, sync mode, entry criteria, timeouts, cancellation policy, and output contract.
- Every dispatch is tagged with run, stage, lane, attempt, and target identity.
- The system captures stdout, stderr, exit status, timing, and terminal health signals.
- Cancellation works at lane, stage, and run scope.
- Retries are explicit, provenance-visible, and never silently widen target scope.
- Partial failure is represented structurally rather than collapsed into generic failure.
- Each stage emits both a lane result vector and a reduced stage aggregate.
- Operator surfaces can distinguish blocked, failed, cancelled, and compensated execution states.
