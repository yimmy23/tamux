# Goal Run Execution Dossier Design

## Goal

Extend tamux goal runners with a first-class structured execution layer that supports live progress tracking, proof-driven verification, richer autonomous resume behavior, and operator-visible artifacts across daemon, CLI, and TUI.

This design intentionally does not clone Droid Mission naming or schema shape. The implementation should preserve tamux's existing goal-run architecture and expose the same user value through tamux-native concepts.

## Summary

The current goal-runner stack already provides:

- durable goal runs in daemon state
- LLM-generated execution plans
- child-task orchestration on the queue
- approvals, replanning, checkpoints, and reflections
- CLI and TUI visibility for run state

What is missing is a structured control layer around the run itself. tamux should add an `execution dossier` per goal run with these core concepts:

- `delivery units`: decomposed work units the run advances through
- `proof checks`: named validations attached to units or milestones
- `evidence records`: structured proof collected from commands, files, summaries, and timestamps
- `run reports`: worker, verifier, reviewer, and orchestrator reports
- `resume decisions`: structured daemon decisions explaining why the run advances, repairs, splits, re-verifies, defers, escalates, or stops

The daemon remains the authority, but every meaningful dossier mutation dual-writes into projected files under `.tamux/goals/<goal-run-id>/` so external scripts and operators can inspect the run live.

## Non-Goals

This pass does not:

- replace the existing goal-run engine with a new mission engine
- make projected JSON the source of truth
- introduce a second independent orchestration system outside the daemon
- require a web UI before shipping the structured dossier layer
- attempt full bidirectional import and conflict reconciliation from hand-edited JSON

## Design Principles

### Preserve tamux's center of gravity

Goal runs already live in daemon-managed durable state. The dossier layer should extend that model, not route execution through file parsing.

### Dual-write from day one

The user requirement is live, externally trackable state. SQLite remains authoritative, but dossier projections must update continuously, not only at the end of the run.

### Do not clone competitor naming

The design should not ship `missions`, `features.json`, or cloned field names. The user value is structured planning, proof tracking, handoff reports, and autonomous continuation. tamux should deliver that with its own vocabulary.

### Verification is composable

Verification must be assignable to built-in agents and user-defined sub-agents. It cannot be hardcoded to one verifier persona.

### Operator stop is first-class

`pause` and `stop` are different control intents. tamux should record and expose both distinctly.

## User-Facing Behavior

When a goal run starts, tamux should:

1. create the normal goal run
2. generate an execution dossier for that run
3. dual-write the initial dossier to SQLite and `.tamux/goals/<goal-run-id>/`
4. execute delivery units through the existing goal-run step machinery
5. collect reports and proof over time
6. decide whether to advance, repair, re-verify, split, defer, escalate, pause, or stop

The operator should be able to:

- inspect dossier state from CLI and TUI
- see proof coverage and verifier bindings live
- stop a goal run from CLI or TUI goal picker
- distinguish paused, stopped, failed, completed, and awaiting-approval states

External tooling should be able to:

- tail or poll `.tamux/goals/<goal-run-id>/dossier.json`
- inspect proof status through `proof-ledger.json`
- consume structured reports from `reports/*.json`

## Dossier Vocabulary

### Delivery Unit

A structured work item derived from the goal plan. It is the tamux-native equivalent of a tracked feature slice, but not named or shaped as a cloned feature object.

Each delivery unit should capture:

- stable unit id
- title and description
- dependency and prerequisite references
- expected outcomes
- linked proof-check ids
- assigned execution role
- assigned verification role
- lifecycle status

### Proof Check

A named validation item that determines whether a unit or milestone can be treated as complete.

Each proof check should capture:

- stable proof-check id
- title and assertion text
- scope: unit-level or milestone-level
- method: command, inspection, report-based, or custom verifier
- current state
- latest evidence summary
- verifier binding

### Evidence Record

A structured proof artifact attached to a proof check. This is broader than raw command output and should support:

- command invocation summary
- file path references
- thread/report references
- verifier summary
- timestamps
- pass/fail/uncertain outcome

### Run Report

A structured report emitted by an execution or verification phase. Reports should support these roles:

- implementer
- verifier
- reviewer
- orchestrator

The report schema should support:

- what changed or was evaluated
- what was left undone
- verification commands run
- discovered issues
- follow-up recommendations
- machine-readable outcome

### Resume Decision

A daemon-authored structured decision recorded after execution or verification events. This makes auto-resume explainable and observable.

## Filesystem Projection

Each goal run gets a live projection directory:

```text
.tamux/goals/<goal-run-id>/
├── goal.md
├── dossier.json
├── proof-ledger.json
├── proof-catalog.md
└── reports/
    ├── implementer-<timestamp>.json
    ├── verifier-<timestamp>.json
    ├── reviewer-<timestamp>.json
    └── orchestrator-<timestamp>.json
```

### File Responsibilities

- `goal.md`: human-readable summary of the run, objective, current state, and key bindings
- `dossier.json`: delivery units, dependencies, role bindings, statuses, current decision snapshot
- `proof-ledger.json`: proof checks, evidence summaries, latest outcomes, coverage state
- `proof-catalog.md`: readable proof definitions and milestone coverage
- `reports/*.json`: append-only structured reports emitted during execution

Projected files are copies, not authority. They should be safe for external inspection and scripting, but daemon state wins on disagreement in this pass.

## Daemon Data Model

The goal-run engine should gain a dossier attachment instead of overloading existing goal step fields.

Recommended top-level additions:

- `GoalRunDossier`
- `GoalDeliveryUnit`
- `GoalProofCheck`
- `GoalEvidenceRecord`
- `GoalRunReport`
- `GoalResumeDecision`
- `GoalRoleBinding`
- `GoalProjectionState`

### GoalRoleBinding

Role bindings should support at minimum:

- `builtin:<agent-id>`
- `subagent:<definition-id>`
- `auto`

This allows verification to target:

- built-in agents
- built-in specialist roles
- user-defined sub-agent definitions

The same binding model should be reusable for execution and review roles.

## Planner Extension

The existing planning flow currently produces generic `GoalRunStep` entries. That should remain the execution backbone, but planning should also create dossier records.

Planning should output:

- run title and summary
- goal-run steps for execution
- delivery units mapped to those steps
- proof checks linked to units or milestones
- initial role bindings for execution and verification
- milestone boundaries where broader proof sweeps are required

Each delivery unit should reference its backing step or step range. This preserves compatibility with the current dispatcher and replan machinery while enabling richer tracking.

## Verification Model

Verification is a first-class orchestration phase, not a loose convention.

Each delivery unit or proof-check group should carry a verifier binding. The orchestrator should resolve that binding and schedule verification work through:

- a built-in verifier-capable agent
- a built-in specialist
- a user-defined sub-agent definition

This must work for both:

- dossier-generated verification created during planning
- operator-configured verifier preferences added later

The orchestrator should record which verifier actually ran and attach its report and evidence to the corresponding proof checks.

## Autonomous Resume Behavior

The orchestrator should classify outcomes against dossier state instead of treating every failure as a human stop point.

Initial resume-decision actions:

- `advance`
- `repair`
- `reverify`
- `split`
- `defer`
- `escalate`
- `milestone_audit`
- `stop`

### Decision Semantics

- `advance`: unit and required proof checks passed
- `repair`: implementation failed, but the same unit should be repaired in place
- `reverify`: implementation is likely acceptable, but proof is incomplete, stale, or flaky
- `split`: the unit is too broad and should be decomposed
- `defer`: dependency or prerequisite is unmet, but this is not a whole-run blocker
- `escalate`: real blocker requiring operator input, secret, environment access, or policy resolution
- `milestone_audit`: broader proof sweep is required before more units advance
- `stop`: operator intentionally ended the run

Only `escalate` should interrupt the operator by default. The rest should stay autonomous and visible.

## Stop, Pause, Resume Semantics

tamux already has goal control actions. This design should extend them with explicit stop semantics.

### Pause

Pause means:

- orchestration is intentionally suspended
- resumability is preserved
- the dossier remains open and active

### Stop

Stop means:

- the operator intentionally ended the run
- the run should not auto-resume
- a structured stop reason is recorded
- the dossier projection reflects a terminal stopped state

`stop` should be available from:

- CLI goal commands
- TUI goal picker actions
- TUI goal detail view

Stop should not be conflated with failure or cancellation caused by internal execution errors.

## CLI Surface

The CLI should keep the existing goal-run interface and add dossier-aware views.

Recommended additions:

- `tamux goal show <id>`
- `tamux goal dossier <id>`
- `tamux goal proof <id>`
- `tamux goal reports <id>`
- `tamux goal stop <id> --reason "<text>"`

Expected behavior:

- `show` includes dossier summary, current decision, and proof coverage
- `dossier` prints structured unit state
- `proof` prints proof-check coverage and verifier/evidence status
- `reports` lists or prints recent reports
- `stop` records an operator stop event and updates both daemon state and projections

## TUI Surface

TUI should expose dossier state as part of the existing goal-run UX, not as a disconnected subsystem.

Initial additions:

- goal picker support for built-in `stop`
- goal detail pane sections for:
  - delivery units
  - proof coverage
  - verifier bindings
  - recent reports
  - latest resume decision
- clearer distinction between:
  - running
  - awaiting approval
  - paused
  - stopped
  - failed
  - completed

The goal picker should not become cluttered. Reuse the existing low-noise action pattern already used for goal controls.

## Projection And Sync Rules

Because this pass dual-writes from day one, projection behavior must be explicit.

### Write Rules

On every meaningful dossier mutation, the daemon should:

1. update SQLite state
2. refresh the corresponding projected JSON/Markdown files
3. append any new run reports
4. emit the normal goal-run/task events for TUI and CLI consumers

### Failure Rules

If file projection fails:

- SQLite remains authoritative
- the goal run should record projection drift in its events or projection state
- the orchestrator should continue unless the operator explicitly marks projection integrity as required

### Future Import

This design leaves room for future import or reconciliation from edited JSON files, but that is not required for the first implementation.

## Suggested Internal File Ownership

Likely daemon areas affected:

- goal-run types
- goal creation and control paths
- goal planner
- goal progress / failure / completion handling
- persistence and projection
- protocol messages for richer goal detail and stop action

Likely CLI areas affected:

- goal command definitions
- bridge/client command wiring
- output formatting for dossier/proof/report views

Likely TUI areas affected:

- goal picker controls
- task/goal state models
- goal detail rendering
- event reducers and modal handling

## Risks

- Dual-write from day one increases surface area for drift and partial-write edge cases.
- Goal detail payloads may exceed IPC caps if dossier and report data are sent naively.
- Verifier bindings to user-defined sub-agents need graceful handling when a referenced definition is missing or removed.
- Stop semantics must remain distinct from pause, cancel, and failure in daemon, CLI, and TUI to avoid user confusion.
- Dossier growth may require pagination or capped windows similar to existing goal-run event pagination.

## Validation

### Daemon

- unit tests for dossier creation and persistence
- unit tests for projection writes and projection-failure behavior
- unit tests for verifier binding resolution across built-in and user-defined bindings
- unit tests for resume-decision classification
- unit tests for pause vs stop semantics
- integration tests for execute -> verify -> advance / repair / reverify / split / escalate loops

### CLI

- command parsing tests for dossier, proof, reports, and stop subcommands
- output tests for dossier and proof summaries
- stop-command tests ensuring the structured stop reason reaches daemon state

### TUI

- reducer tests for dossier-rich goal detail updates
- goal picker tests for stop action availability and confirmation handling
- rendering tests for proof coverage, verifier binding, and resume-decision sections
- regression tests preserving existing goal list/detail behavior under IPC-capped responses

## Recommendation

Implement the execution dossier as an extension of the existing goal-run engine, with SQLite as authority and live dual-written JSON/Markdown projections under `.tamux/goals/<goal-run-id>/`.

This delivers the structured workflow layer the user needs while preserving tamux's own architecture, naming, and operator model.
