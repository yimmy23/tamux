# Goal Mission Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current goal composer/detail flow with a first-class Mission Control workflow that supports preflight model selection, runtime agent editing, active-thread steering, and active-thread-driven header metadata.

**Architecture:** Add daemon-to-TUI data plumbing for goal launch snapshots, runtime agent assignments, and active/root/execution thread routing first. Then build a dedicated Mission Control state/render path in the TUI, replace the thin goal composer with a preflight surface, add runtime agent-roster editing and thread routing controls, and finally rebind the header to the active execution thread with explicit fallback states.

**Tech Stack:** Rust workspace, `amux-daemon`, `amux-tui`, Ratatui, Crossterm, Tokio channels, serde, existing TUI state/widget split

---

## File Map

### Daemon Goal Data And Persistence

- Modify: `crates/amux-daemon/src/agent/types/goal_types.rs`
  Add goal launch snapshot, runtime assignment, and thread-routing fields to daemon goal-run types.
- Modify: `crates/amux-daemon/src/agent/goal_planner.rs`
  Seed launch snapshot defaults and initial thread-routing metadata when a goal is created.
- Modify: `crates/amux-daemon/src/agent/goal_planner/progress.rs`
  Update active execution thread and runtime ownership as work moves between steps/threads.
- Modify: `crates/amux-daemon/src/agent/task_crud.rs`
  Preserve the new fields when goal detail payloads are windowed/sliced.
- Modify: `crates/amux-daemon/src/history/schema_migrations.rs`
  Add any goal-run persistence columns needed for launch/runtime assignment snapshots.
- Modify: `crates/amux-daemon/src/history/goal_runs.rs`
  Persist and reload launch/runtime assignment snapshot data if goal runs are stored there.
- Test: `crates/amux-daemon/src/agent/tests/goal_planner.rs`
  Validate launch defaults and active-thread metadata on emitted goal runs.
- Test: `crates/amux-daemon/src/agent/tests/task_crud.rs`
  Validate new goal-run metadata survives capped/detail payload generation.
- Test: `crates/amux-daemon/src/history/tests/goal_runs.rs`
  Validate snapshot persistence and reload behavior.

### Shared Wire And TUI Goal State

- Modify: `crates/amux-tui/src/wire.rs`
  Parse goal launch snapshot, runtime assignment, and thread-routing fields.
- Modify: `crates/amux-tui/src/app/conversion.rs`
  Convert wire goal-run metadata into TUI task state.
- Modify: `crates/amux-tui/src/state/task.rs`
  Extend `GoalRun` with mission-control metadata and merge logic.
- Modify: `crates/amux-tui/src/state/mod.rs`
  Extend daemon command types if TUI must send runtime assignment edits or preflight launch payloads.
- Test: `crates/amux-tui/src/state/tests/task.rs`
  Validate merge/fallback rules for new goal metadata.

### Mission Control Pane And Navigation

- Modify: `crates/amux-tui/src/app/mod.rs`
  Add explicit Mission Control / preflight pane state and return-anchor structs.
- Modify: `crates/amux-tui/src/app/commands.rs`
  Replace the thin `GoalComposer` start flow with preflight open/start helpers and thread-return helpers.
- Modify: `crates/amux-tui/src/app/model_impl_part2.rs`
  Add navigation helpers for Mission Control, `/threads` jump, and return-to-goal behavior.
- Modify: `crates/amux-tui/src/app/keyboard.rs`
  Route Mission Control keyboard actions and prevent goal panes from collapsing into conversation.
- Modify: `crates/amux-tui/src/app/keyboard_enter.rs`
  Submit goal preflight launches instead of raw prompt-based `StartGoalRun`.
- Modify: `crates/amux-tui/src/app/mouse_helpers.rs`
  Handle Mission Control click targets for thread routing and runtime agent edits.
- Modify: `crates/amux-tui/src/app/mouse.rs`
  Preserve click-up/down handling for Mission Control controls.
- Test: `crates/amux-tui/src/app/tests/tests_part6.rs`
  Validate goal/thread navigation, jump-to-thread, and return-to-goal behavior.

### Mission Control State And Rendering

- Create: `crates/amux-tui/src/state/goal_mission_control.rs`
  Focused state for preflight selection, runtime roster editing, pending changes, and return anchors.
- Modify: `crates/amux-tui/src/state/mod.rs`
  Export the new mission-control state types.
- Create: `crates/amux-tui/src/widgets/goal_mission_control.rs`
  Render Mission Control regions and expose hit-test targets.
- Create: `crates/amux-tui/src/widgets/tests/goal_mission_control.rs`
  Dedicated render/hit-test tests for Mission Control.
- Modify: `crates/amux-tui/src/app/rendering.rs`
  Render Mission Control in place of the current thin goal composer / plain goal pane stack.
- Modify: `crates/amux-tui/src/app/render_helpers.rs`
  Remove or reduce the current `GoalComposer` helper once Mission Control preflight is in place.
- Test: `crates/amux-tui/src/app/tests/tests_part5.rs`
  Validate preflight rendering, runtime roster rendering, and thread-router rendering.

### Header Rebinding

- Modify: `crates/amux-tui/src/app/rendering.rs`
  Resolve header profile and usage from active execution thread first when Mission Control is focused.
- Modify: `crates/amux-tui/src/widgets/header.rs`
  Add an explicit fallback/live indicator if needed by the new header context model.
- Test: `crates/amux-tui/src/app/tests/events.rs`
  Validate active-thread-first header profile and context-window fallback order.

### Documentation

- Modify: `docs/superpowers/specs/2026-04-20-goal-mission-control-design.md`
  Keep spec in sync if implementation-driven clarifications are needed.
- Modify: `docs/goal-runners.md`
  Document Mission Control, launch defaults, runtime edits, and thread steering.

## Task 1: Add Goal Mission Control Metadata To Daemon Goal Runs

**Files:**
- Modify: `crates/amux-daemon/src/agent/types/goal_types.rs`
- Modify: `crates/amux-daemon/src/agent/goal_planner.rs`
- Modify: `crates/amux-daemon/src/agent/goal_planner/progress.rs`
- Test: `crates/amux-daemon/src/agent/tests/goal_planner.rs`

- [ ] **Step 1: Write the failing daemon tests for launch snapshot and thread-routing fields**

Add focused tests asserting a new goal run exposes:
- launch assignment snapshot defaults,
- `root_thread_id`,
- `active_thread_id`,
- and an initial execution-thread list.

- [ ] **Step 2: Run the focused daemon tests to verify they fail**

Run: `cargo test -p tamux-daemon goal_planner -- --nocapture`
Expected: FAIL with missing fields or assertions showing goal runs only expose a single `thread_id`.

- [ ] **Step 3: Add new serializable goal-run metadata types and fields**

Implement minimal types in `goal_types.rs`, for example:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GoalAgentAssignment {
    pub role_id: String,
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub inherit_from_main: bool,
}
```

Add goal-run fields for:
- launch assignment snapshot,
- runtime assignment list,
- `root_thread_id`,
- `active_thread_id`,
- `execution_thread_ids`.

- [ ] **Step 4: Seed launch snapshot and initial thread-routing metadata in goal creation**

Use existing runtime/default provider+model resolution in `goal_planner.rs` so a new goal has a stable launch snapshot and initial root/active thread values.

- [ ] **Step 5: Update active execution thread as goal progress changes**

In `goal_planner/progress.rs`, set `active_thread_id` whenever execution moves to a step/task/thread owned by a different agent, and keep the execution-thread set/list current.

- [ ] **Step 6: Re-run the focused daemon tests**

Run: `cargo test -p tamux-daemon goal_planner -- --nocapture`
Expected: PASS for the new launch-snapshot and active-thread assertions.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-daemon/src/agent/types/goal_types.rs \
        crates/amux-daemon/src/agent/goal_planner.rs \
        crates/amux-daemon/src/agent/goal_planner/progress.rs \
        crates/amux-daemon/src/agent/tests/goal_planner.rs
git commit -m "feat: add mission control metadata to goal runs"
```

## Task 2: Persist And Serialize Goal Mission Control Metadata

**Files:**
- Modify: `crates/amux-daemon/src/agent/task_crud.rs`
- Modify: `crates/amux-daemon/src/history/schema_migrations.rs`
- Modify: `crates/amux-daemon/src/history/goal_runs.rs`
- Test: `crates/amux-daemon/src/agent/tests/task_crud.rs`
- Test: `crates/amux-daemon/src/history/tests/goal_runs.rs`

- [ ] **Step 1: Write the failing serialization and persistence tests**

Add tests that:
- capped goal detail retains launch/runtime assignment and active-thread fields,
- saved goal runs reload with the new fields intact.

- [ ] **Step 2: Run the focused daemon tests to verify they fail**

Run: `cargo test -p tamux-daemon task_crud history::tests::goal_runs -- --nocapture`
Expected: FAIL because the new mission-control metadata is dropped on serialization or not persisted.

- [ ] **Step 3: Preserve new fields in goal detail payload generation**

Update `task_crud.rs` windowing/slicing helpers so they carry the new goal-run metadata through authoritative detail and incremental update payloads.

- [ ] **Step 4: Add persistence columns and read/write plumbing**

Store launch/runtime assignment snapshots and active/root thread routing in goal-run persistence, using JSON columns if that matches existing owner-profile storage patterns.

- [ ] **Step 5: Re-run the focused daemon tests**

Run: `cargo test -p tamux-daemon task_crud history::tests::goal_runs -- --nocapture`
Expected: PASS for metadata preservation and reload behavior.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/task_crud.rs \
        crates/amux-daemon/src/history/schema_migrations.rs \
        crates/amux-daemon/src/history/goal_runs.rs \
        crates/amux-daemon/src/agent/tests/task_crud.rs \
        crates/amux-daemon/src/history/tests/goal_runs.rs
git commit -m "feat: persist goal mission control metadata"
```

## Task 3: Extend TUI Wire And Task State For Mission Control Metadata

**Files:**
- Modify: `crates/amux-tui/src/wire.rs`
- Modify: `crates/amux-tui/src/app/conversion.rs`
- Modify: `crates/amux-tui/src/state/task.rs`
- Test: `crates/amux-tui/src/state/tests/task.rs`

- [ ] **Step 1: Write the failing TUI state tests**

Add tests covering:
- parsing of launch/runtime assignment metadata,
- merge behavior across `GoalRunDetailReceived` and `GoalRunUpdate`,
- preservation of active/root/execution thread IDs,
- and fallback when incremental payloads omit some fields.

- [ ] **Step 2: Run the focused TUI state tests to verify they fail**

Run: `cargo test -p tamux-tui state::tests::task -- --nocapture`
Expected: FAIL with missing goal-run fields or merge assertions showing values are lost.

- [ ] **Step 3: Extend `wire::GoalRun` with mission-control fields**

Mirror the daemon fields in `wire.rs`.

- [ ] **Step 4: Extend `state::task::GoalRun` and merge logic**

Store the new fields in `state/task.rs` and update merge helpers so authoritative detail replaces missing data correctly while incremental updates preserve stable values.

- [ ] **Step 5: Update wire-to-state conversion**

Map the new metadata in `app/conversion.rs`.

- [ ] **Step 6: Re-run the focused TUI state tests**

Run: `cargo test -p tamux-tui state::tests::task -- --nocapture`
Expected: PASS for new parsing and merge cases.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/wire.rs \
        crates/amux-tui/src/app/conversion.rs \
        crates/amux-tui/src/state/task.rs \
        crates/amux-tui/src/state/tests/task.rs
git commit -m "feat: teach tui goal state about mission control metadata"
```

## Task 4: Replace GoalComposer With Mission Control Preflight State

**Files:**
- Create: `crates/amux-tui/src/state/goal_mission_control.rs`
- Modify: `crates/amux-tui/src/state/mod.rs`
- Modify: `crates/amux-tui/src/app/mod.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Modify: `crates/amux-tui/src/app/keyboard_enter.rs`
- Modify: `crates/amux-tui/src/app/render_helpers.rs`
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part2.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part5.rs`

- [ ] **Step 1: Write the failing preflight tests**

Add tests asserting:
- opening a new goal goes to Mission Control preflight instead of a plain text composer,
- previous goal settings are loaded as defaults when present,
- fallback is main-agent inheritance when no previous snapshot exists,
- pressing Enter launches from preflight, not directly from raw input mode.

- [ ] **Step 2: Run the focused TUI tests to verify they fail**

Run: `cargo test -p tamux-tui goal_composer mission_control_preflight -- --nocapture`
Expected: FAIL because the app still renders the old `GoalComposer` and directly sends `StartGoalRun`.

- [ ] **Step 3: Add focused Mission Control state**

Create a state module that tracks:
- preflight prompt text,
- selected main model/provider/reasoning,
- role assignments,
- preset source label,
- pending save-as-default toggle.

- [ ] **Step 4: Rewire `open_new_goal_view` and launch submission**

Replace the current `GoalComposer` flow in `commands.rs` and `keyboard_enter.rs` so opening a goal enters Mission Control preflight and submitting sends the structured launch payload.

- [ ] **Step 5: Render the preflight surface**

Render stable Mission Control preflight sections instead of the current helper text in `render_helpers.rs` / `rendering.rs`.

- [ ] **Step 6: Re-run the focused TUI tests**

Run: `cargo test -p tamux-tui goal_composer mission_control_preflight -- --nocapture`
Expected: PASS for preflight open/default/launch behavior.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/state/goal_mission_control.rs \
        crates/amux-tui/src/state/mod.rs \
        crates/amux-tui/src/app/mod.rs \
        crates/amux-tui/src/app/commands.rs \
        crates/amux-tui/src/app/keyboard_enter.rs \
        crates/amux-tui/src/app/render_helpers.rs \
        crates/amux-tui/src/app/rendering.rs \
        crates/amux-tui/src/app/tests/tests_part2.rs \
        crates/amux-tui/src/app/tests/tests_part5.rs
git commit -m "feat: add mission control preflight for goals"
```

## Task 5: Add Mission Control Thread Routing And Return-To-Goal Navigation

**Files:**
- Modify: `crates/amux-tui/src/app/mod.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Modify: `crates/amux-tui/src/app/model_impl_part2.rs`
- Modify: `crates/amux-tui/src/app/keyboard.rs`
- Modify: `crates/amux-tui/src/app/mouse_helpers.rs`
- Create: `crates/amux-tui/src/widgets/goal_mission_control.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part6.rs`
- Test: `crates/amux-tui/src/widgets/tests/goal_mission_control.rs`

- [ ] **Step 1: Write the failing navigation tests**

Add tests asserting:
- `Open active thread` resolves `active_thread_id` first,
- falling back to `root_thread_id` sets an explicit status message,
- `/threads` opened from Mission Control exposes `Return to goal`,
- returning restores the source goal run and step selection.

- [ ] **Step 2: Run the focused navigation tests to verify they fail**

Run: `cargo test -p tamux-tui goal_sidebar_task threads_return_to_goal mission_control_thread_router -- --nocapture`
Expected: FAIL because no Mission Control thread-router or return anchor exists yet.

- [ ] **Step 3: Add return-anchor state and open-thread helpers**

In app state, track the source goal run and step when Mission Control opens `/threads`. Add helpers to open `active_thread_id` or `root_thread_id`.

- [ ] **Step 4: Add Mission Control thread-router controls**

Render `Open active thread` and `Return to goal` hit targets in the Mission Control widget, and wire keyboard/mouse activation to the new helpers.

- [ ] **Step 5: Re-run the focused navigation tests**

Run: `cargo test -p tamux-tui goal_sidebar_task threads_return_to_goal mission_control_thread_router -- --nocapture`
Expected: PASS for active-thread jump and return-to-goal behavior.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-tui/src/app/mod.rs \
        crates/amux-tui/src/app/commands.rs \
        crates/amux-tui/src/app/model_impl_part2.rs \
        crates/amux-tui/src/app/keyboard.rs \
        crates/amux-tui/src/app/mouse_helpers.rs \
        crates/amux-tui/src/widgets/goal_mission_control.rs \
        crates/amux-tui/src/app/tests/tests_part6.rs \
        crates/amux-tui/src/widgets/tests/goal_mission_control.rs
git commit -m "feat: add mission control thread routing"
```

## Task 6: Add Runtime Agent Roster Editing And Confirmed Reassignment

**Files:**
- Modify: `crates/amux-tui/src/state/goal_mission_control.rs`
- Modify: `crates/amux-tui/src/app/mod.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers_enter.rs`
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Modify: `crates/amux-tui/src/widgets/goal_mission_control.rs`
- Modify: `crates/amux-tui/src/state/mod.rs`
- Test: `crates/amux-tui/src/app/tests/modal_handlers.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part5.rs`

- [ ] **Step 1: Write the failing runtime-roster tests**

Add tests asserting:
- agent/provider/model/role edits are possible during a run,
- edits default to future-turn-only,
- active-step reassignment/restart requires explicit confirmation,
- Mission Control shows `live now` vs `pending next turn`.

- [ ] **Step 2: Run the focused TUI tests to verify they fail**

Run: `cargo test -p tamux-tui goal_view_action_menu runtime_assignment mission_control_roster -- --nocapture`
Expected: FAIL because no runtime roster editor or pending-change state exists.

- [ ] **Step 3: Add runtime assignment edit state and commands**

Track in-progress edits, pending future-turn assignments, and the current disruptive-action choice.

- [ ] **Step 4: Add confirmation flow for active-step impact**

Use existing modal patterns to ask whether to:
- apply next turn,
- reassign active step,
- or restart/requeue active step.

- [ ] **Step 5: Render editable agent roster controls**

Expose row-level controls in Mission Control for provider/model/reasoning/role/enabled/inherit fields, staying within the repo’s focused-file-size rules by splitting widget helpers if needed.

- [ ] **Step 6: Re-run the focused TUI tests**

Run: `cargo test -p tamux-tui goal_view_action_menu runtime_assignment mission_control_roster -- --nocapture`
Expected: PASS for runtime-edit safety behavior.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/state/goal_mission_control.rs \
        crates/amux-tui/src/app/mod.rs \
        crates/amux-tui/src/app/commands.rs \
        crates/amux-tui/src/app/modal_handlers.rs \
        crates/amux-tui/src/app/modal_handlers_enter.rs \
        crates/amux-tui/src/app/rendering.rs \
        crates/amux-tui/src/widgets/goal_mission_control.rs \
        crates/amux-tui/src/state/mod.rs \
        crates/amux-tui/src/app/tests/modal_handlers.rs \
        crates/amux-tui/src/app/tests/tests_part5.rs
git commit -m "feat: add runtime agent editing in mission control"
```

## Task 7: Rebind Mission Control Header To Active Execution Thread

**Files:**
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Modify: `crates/amux-tui/src/widgets/header.rs`
- Test: `crates/amux-tui/src/app/tests/events.rs`
- Test: `crates/amux-tui/src/widgets/header.rs`

- [ ] **Step 1: Write the failing header tests**

Add tests covering Mission Control header resolution order:
1. `active_thread_id`
2. `root_thread_id`
3. launch assignment snapshot
4. generic config defaults

Also assert the shown context window changes when the active execution thread changes.

- [ ] **Step 2: Run the focused header tests to verify they fail**

Run: `cargo test -p tamux-tui header_goal_run mission_control_header -- --nocapture`
Expected: FAIL because the current code still uses `thread_id` and owner-profile fallback rather than an explicit active-thread-first resolution.

- [ ] **Step 3: Replace goal-pane header resolvers with active-thread-first logic**

Update `goal_run_header_profile`, `goal_run_usage_thread`, and related helpers in `rendering.rs` to read Mission Control thread-routing metadata first.

- [ ] **Step 4: Add explicit fallback/live indication if required**

If the current header widget cannot communicate fallback state cleanly, add a minimal indicator in `widgets/header.rs` rather than a full redesign.

- [ ] **Step 5: Re-run the focused header tests**

Run: `cargo test -p tamux-tui header_goal_run mission_control_header -- --nocapture`
Expected: PASS for active-thread-driven profile and context-window behavior.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-tui/src/app/rendering.rs \
        crates/amux-tui/src/widgets/header.rs \
        crates/amux-tui/src/app/tests/events.rs
git commit -m "fix: bind mission control header to active execution thread"
```

## Task 8: Finalize Mission Control Layout And Regression Coverage

**Files:**
- Modify: `crates/amux-tui/src/widgets/goal_mission_control.rs`
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Modify: `crates/amux-tui/src/app/tests/tests_part5.rs`
- Modify: `crates/amux-tui/src/app/tests/tests_part6.rs`
- Create or modify: `crates/amux-tui/src/widgets/tests/goal_mission_control.rs`
- Modify: `docs/goal-runners.md`
- Modify: `docs/superpowers/specs/2026-04-20-goal-mission-control-design.md`

- [ ] **Step 1: Write any remaining failing render/hit-test regressions**

Cover:
- preflight sections,
- runtime roster layout,
- execution feed,
- thread router,
- return-to-goal affordance,
- and non-collapsing goal/thread separation.

- [ ] **Step 2: Run the focused TUI regression slices to verify status**

Run: `cargo test -p tamux-tui goal_view_ task_view_ mission_control_ -- --nocapture`
Expected: any remaining FAILs point to missing layout or interaction coverage.

- [ ] **Step 3: Finish layout polish and docs**

Ensure Mission Control renders as stable operator panels rather than a plain stacked transcript, and document the new workflow in `docs/goal-runners.md`.

- [ ] **Step 4: Run formatting and the main verification suite**

Run:

```bash
cargo fmt --all
cargo test -p tamux-daemon goal_planner task_crud history::tests::goal_runs -- --nocapture
cargo test -p tamux-tui goal_view_ task_view_ mission_control_ header_goal_run -- --nocapture
```

Expected:
- `cargo fmt --all` succeeds with no diff afterward.
- Focused daemon and TUI slices pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-tui/src/widgets/goal_mission_control.rs \
        crates/amux-tui/src/app/rendering.rs \
        crates/amux-tui/src/app/tests/tests_part5.rs \
        crates/amux-tui/src/app/tests/tests_part6.rs \
        crates/amux-tui/src/widgets/tests/goal_mission_control.rs \
        docs/goal-runners.md \
        docs/superpowers/specs/2026-04-20-goal-mission-control-design.md
git commit -m "feat: complete goal mission control workflow"
```

## Notes For Execution

- Keep new files under 500 LOC; split widget/state helpers early rather than letting `goal_mission_control.rs` become another monolith.
- Prefer extending existing modal and picker patterns for runtime editing instead of inventing a second interaction system.
- Do not silently fall back from Goals to Conversation anywhere in this work; that violates the approved interaction model.
- When tests reference new command payload shapes, update only the minimal necessary wire and conversion points first so failures stay localized.
