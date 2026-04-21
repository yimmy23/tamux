# Goal Mission Control Redesign

Date: 2026-04-21
Area: `crates/amux-tui`
Status: Approved design, not yet implemented

## Goal

Redesign the full goal mission control screen so it becomes readable and operational during active goal runs. The current screen feels fragmented because launch setup, execution output, and goal navigation are split across multiple surfaces and peer tabs. The redesign should create one coherent live workspace.

The key product requirement is:

- Replace the separate `Steps` and `Tasks` tabs with a single `Plan` view.
- Show steps as top-level items with nested todo beneath each step.
- Keep the live execution feed visible at the same time.
- Move files and checkpoints out of top-level peer tabs and into contextual details for the current selection.

The target interaction style is balanced rather than log-first or plan-first: the plan tree and execution feed should share the screen, with neither hidden behind a mode switch.

## Current Problems

The current goal mission control experience is split across multiple surfaces:

- A preflight mission control widget in [`crates/amux-tui/src/widgets/goal_mission_control.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/goal_mission_control.rs:1)
- A goal sidebar with `Steps`, `Checkpoints`, `Tasks`, and `Files` tabs in [`crates/amux-tui/src/widgets/goal_sidebar.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/goal_sidebar.rs:1)
- A task/goal detail renderer in [`crates/amux-tui/src/widgets/task_view.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/task_view.rs:1) and [`crates/amux-tui/src/widgets/task_view_sections.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/task_view_sections.rs:1)

This creates several UX failures:

- The user must mentally merge plan state, execution state, and artifacts from different visual areas.
- `Steps` and `Tasks` overlap conceptually, which makes the model harder to understand than the underlying work.
- Files and checkpoints are important, but not important enough to deserve equal top-level navigation status with the plan itself.
- The event feed is technically rich but visually noisy because category labels and status labels compete with the actual information.

## Design Summary

The redesigned screen is a single three-column workspace with a compact summary row above it and a command bar below it.

### Top Summary Row

The top row shows concise status cards:

- `Goal`
- `Progress`
- `Active agent`
- `Needs attention`

This row replaces repeated status noise scattered through the body. It is responsible for surfacing whether the run is active, paused, awaiting approval, failed, or complete.

### Main Body

The main body is split into three persistent panes:

1. `Plan`
2. `Run timeline`
3. `Details`

#### Plan Pane

The left pane is the primary navigator.

- Top-level rows are goal steps.
- Nested rows under each step are todo items associated with that step.
- There is no separate `Tasks` tab in the redesigned goal screen.
- A step can be collapsed or expanded.
- The selected step or todo drives the content of the `Details` pane.

The pane should feel like a plan tree rather than a generic list. It is the main answer to “what is the agent doing and what remains?”

#### Run Timeline Pane

The center pane is a readable live event stream.

- It remains visible at all times.
- It is rendered as grouped timeline entries rather than a dense raw log.
- Event language should be calmer and more human-readable.
- Prefix noise should be reduced when the pane context already provides category information.

Example event names:

- `Planning complete`
- `Approval requested`
- `Step 2 started`
- `Todo updated`
- `Verification queued`

This pane is primarily for situational awareness. It should stay usable as a secondary interactive surface, but it should not replace the plan tree as the main navigator.

#### Details Pane

The right pane is contextual, not global.

- It shows files relevant to the current selection.
- It shows checkpoints relevant to the current selection.
- It may show selected-step metadata, approval state, or shortcut hints.

Files and checkpoints move here from the current top-level tabs. They are no longer peer navigation modes for the entire screen.

### Bottom Command Bar

The bottom row remains the operator input and action surface.

It should support the existing command/composer role for:

- operator replies
- approvals
- retries
- open-thread actions
- slash commands

## Information Architecture Decisions

### 1. Remove the `Tasks` Tab

The current `Tasks` tab is conceptually confusing because it competes with `Steps`.

In the redesign:

- steps remain the primary units of the plan
- step-owned todo is nested directly under each step
- child tasks are treated as execution detail, not as top-level navigation peers

If child task information must remain accessible, it should appear in the timeline and details surfaces, not as a primary tab next to the plan.

### 2. Remove Top-Level `Checkpoints` and `Files` Tabs

Checkpoints and files are secondary context.

In the redesign:

- checkpoints become part of the selected step’s details
- files become part of the selected step’s details
- the screen no longer asks the user to switch entire navigation modes just to inspect supporting artifacts

### 3. Make the Plan Pane the Default Focus

The screen should open with the plan tree as the primary focus target.

Reasoning:

- the plan is the clearest mental model for the run
- it anchors both execution and details
- it is the best place to unify step selection and nested todo navigation

The timeline remains visible and informative, but it is not the primary cursor target when the screen first opens.

## Interaction Model

The redesigned screen should operate around one dominant selection model.

### Primary Selection

The active selection lives in the `Plan` pane.

- selecting a step highlights that step and updates `Details`
- selecting a nested todo keeps its parent step active in the plan hierarchy while shifting `Details` toward that todo context
- the timeline may reflect the selected step when possible, but it does not own the primary state

### Navigation

Navigation should stay simple and predictable:

- up/down moves through visible tree rows
- right expands a collapsed step
- left collapses an expanded step or moves back to the parent step context
- tab cycles pane focus between `Plan`, `Run timeline`, `Details`, and the command bar
- enter opens the selected detail target when that target is actionable

### Empty and Failure States

The redesign should keep exceptional states explicit but visually calm.

#### No Plan Yet

- `Plan` shows a single waiting state such as `Waiting for plan`
- `Run timeline` continues showing planning events
- `Details` shows a neutral empty state

#### No Files or Checkpoints for Selection

- `Details` shows an empty contextual state
- the user is not pushed into another tab or mode

#### Hold, Pause, Failure

- the summary row surfaces the state at a glance
- the timeline highlights the relevant event
- the plan remains stable as the main navigator

## Mapping to Existing TUI Structures

This redesign should be implemented primarily as a re-composition of existing data rather than a protocol rewrite.

### Existing Structures to Reuse

- `GoalRunStep` from the task state is the basis for top-level plan rows.
- goal-run events already support a live activity/timeline surface.
- goal-run checkpoints already exist and can be rendered contextually.
- work-context/file entries already exist and can be rendered contextually.

Relevant code paths:

- [`crates/amux-tui/src/state/goal_sidebar.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/state/goal_sidebar.rs:1)
- [`crates/amux-tui/src/widgets/goal_sidebar.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/goal_sidebar.rs:1)
- [`crates/amux-tui/src/widgets/task_view.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/task_view.rs:1)
- [`crates/amux-tui/src/widgets/task_view_sections.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/task_view_sections.rs:1)
- [`crates/amux-tui/src/app/tests/tests_part6.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/app/tests/tests_part6.rs:1)

### Expected Structural Changes

The implementation should likely:

- replace `GoalSidebarTab`-driven rendering with a tree-oriented `Plan` pane state model
- move goal-run-specific files and checkpoints out of tab navigation and into a contextual details renderer
- reshape the existing live activity output into a cleaner timeline renderer
- preserve the underlying goal-run/task data flows already handled by the app and event reducers

This is intended to be a UI-state and rendering refactor, not a backend schema redesign.

## Copy and Labeling Guidelines

Language should become quieter and more intentional.

Preferred labels:

- `Plan` instead of competing `Steps` and `Tasks`
- `Run timeline` instead of a noisy execution log framing
- `Details` instead of peer tabs for supporting entities
- `Needs attention` for actionable hold/approval summary

Avoid:

- repeated bracketed category prefixes when the containing pane already communicates category
- multiple labels that describe nearly the same concept
- exposing internal terminology as primary UI language unless it helps the operator

## Testing Strategy

The existing ratatui render-test approach should be extended for the redesign.

Required test coverage:

- render tests for the full three-pane goal mission control screen
- navigation tests for moving through steps and nested todo
- tests for expand/collapse behavior in the plan tree
- empty-state tests for no plan, no details, and no events
- regression tests proving files and checkpoints are contextual details rather than top-level goal tabs

Tests should be added close to the current goal sidebar and goal task view coverage, especially in:

- [`crates/amux-tui/src/widgets/tests/goal_sidebar.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/widgets/tests/goal_sidebar.rs:1)
- [`crates/amux-tui/src/app/tests/tests_part6.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-tui/src/app/tests/tests_part6.rs:1)

## Non-Goals

The redesign does not require:

- changing daemon or protocol payload shapes unless implementation proves a concrete gap
- inventing a brand-new goal entity model
- turning the timeline into the default operator surface
- preserving the existing four-tab goal sidebar metaphor

## Recommended Implementation Direction

Use the approved hybrid direction:

- keep the stable three-column workspace from the split-workspace concept
- borrow the cleaner wording and calmer timeline treatment from the timeline-centered concept
- anchor the whole screen around a single plan tree with nested todo

This design should be used as the basis for the implementation plan. No implementation should preserve the current `Steps / Checkpoints / Tasks / Files` goal sidebar model unchanged.
