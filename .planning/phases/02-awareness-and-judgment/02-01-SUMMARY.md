---
phase: 02-awareness-and-judgment
plan: 01
subsystem: agent-awareness
tags: [situational-awareness, sliding-window, trajectory, mode-shift, counter-who]

# Dependency graph
requires:
  - phase: 01-memory-foundation
    provides: "EpisodicStore, CounterWhoState, counter_who::compute_approach_hash, counter_who::detect_repeated_approaches"
provides:
  - "AwarenessMonitor with per-entity 3-tier sliding window outcome tracking"
  - "Diminishing returns detection with counter-who false positive guard"
  - "Trajectory computation (converging/diverging/stalled)"
  - "Mode shift evaluation and notification via AgentEvent"
  - "TrajectoryUpdate, ModeShift, ConfidenceWarning AgentEvent variants"
  - "aggregate_short_term_success_rate() for confidence scoring (Plan 03)"
affects: [02-02, 02-03, confidence-scoring, goal-runner]

# Tech tracking
tech-stack:
  added: []
  patterns: ["3-tier sliding window (short-term 5 actions, medium-term 30 min, long-term session)", "dual-signal mode shift (awareness + counter-who)", "entity-capped HashMap with LRU eviction"]

key-files:
  created:
    - "crates/amux-daemon/src/agent/awareness/mod.rs"
    - "crates/amux-daemon/src/agent/awareness/tracker.rs"
    - "crates/amux-daemon/src/agent/awareness/trajectory.rs"
    - "crates/amux-daemon/src/agent/awareness/mode_shift.rs"
  modified:
    - "crates/amux-daemon/src/agent/mod.rs"
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/agent/engine.rs"
    - "crates/amux-daemon/src/agent/agent_loop.rs"
    - "crates/amux-daemon/src/agent/heartbeat_checks.rs"

key-decisions:
  - "Diminishing returns threshold: 3+ consecutive same-pattern calls with <30% short-term success rate"
  - "Counter-who consulted before ALL mode shifts (AWAR-03 locked decision)"
  - "Progress heuristic: non-error results with >50 chars content indicate new information gained"
  - "Trajectory stalled zone: ratio between -0.1 and 0.1 treated as stalled"
  - "aggregate_short_term_success_rate returns 0.8 when no windows exist (healthy default for confidence scoring)"

patterns-established:
  - "awareness/ module follows same pattern as episodic/: data types + AgentEngine impl block in same module"
  - "Entity-capped HashMap with MAX_TRACKED_ENTITIES=100 and LRU eviction on overflow"
  - "Dual-signal guard pattern: awareness detects + counter-who confirms before action fires"

requirements-completed: [AWAR-01, AWAR-02, AWAR-03, AWAR-04, AWAR-05]

# Metrics
duration: 11min
completed: 2026-03-27
---

# Phase 02 Plan 01: Situational Awareness Summary

**Per-entity 3-tier sliding window awareness with diminishing returns detection, counter-who false positive guard, trajectory computation, and mode shift notification**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-27T07:26:59Z
- **Completed:** 2026-03-27T07:38:19Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- AwarenessMonitor tracks tool outcomes per-entity with three-tier sliding windows (5 actions / 30 min / session)
- Diminishing returns detection fires after 3+ same tool+args pattern with <30% success rate
- Mode shift requires BOTH diminishing returns AND counter-who confirmation (AWAR-03 guard prevents false positives)
- Trajectory computation provides converging/diverging/stalled direction from progress vs failure ratio
- 3 new AgentEvent variants (TrajectoryUpdate, ModeShift, ConfidenceWarning) enable broadcasting to all clients
- Every tool call in agent_loop now records an awareness outcome and evaluates mode shift

## Task Commits

Each task was committed atomically:

1. **Task 1: Create awareness module types, tracker, trajectory, and mode_shift logic** - `1803e51` (feat)
2. **Task 2: Wire awareness into AgentEngine and agent_loop** - `1fb8925` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/awareness/mod.rs` - AwarenessMonitor struct, per-entity tracking, AgentEngine integration helpers
- `crates/amux-daemon/src/agent/awareness/tracker.rs` - OutcomeWindow and OutcomeEntry with 3-tier rate computation
- `crates/amux-daemon/src/agent/awareness/trajectory.rs` - TrajectoryState and compute_trajectory from progress/failure counts
- `crates/amux-daemon/src/agent/awareness/mode_shift.rs` - ModeShiftDecision with dual-signal (awareness + counter-who) guard
- `crates/amux-daemon/src/agent/mod.rs` - Added `pub mod awareness;` declaration
- `crates/amux-daemon/src/agent/types.rs` - Added TrajectoryUpdate, ModeShift, ConfidenceWarning AgentEvent variants
- `crates/amux-daemon/src/agent/engine.rs` - Added awareness field to AgentEngine struct and initialization
- `crates/amux-daemon/src/agent/agent_loop.rs` - Wired record_awareness_outcome and check_awareness_mode_shift after tool calls
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Updated test constructor with awareness field

## Decisions Made
- Diminishing returns uses dual threshold: consecutive_same_pattern >= 3 AND short_term_success_rate < 0.3
- Counter-who is ALWAYS consulted before mode shifts fire (locked decision AWAR-03)
- Progress heuristic: non-error results with >50 chars content = new information gained
- Trajectory stalled zone: ratio between -0.1 and 0.1 classified as stalled
- Default aggregate success rate of 0.8 when no windows exist (healthy assumption for Plan 03 confidence scoring)
- Strategy rotation pool: ["try different tool", "broaden search", "ask operator", "simplify approach"]

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- detect_repeated_approaches actual signature takes `&[TriedApproach]` not `&CounterWhoState` as the plan's interface section suggested. Adapted the mode_shift logic to access `store.counter_who.tried_approaches` directly. No impact on behavior.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Awareness module ready for Plan 02 (uncertainty quantification) to consume aggregate_short_term_success_rate
- TrajectoryUpdate, ModeShift, ConfidenceWarning events ready for TUI/GUI rendering
- Mode shift notifications will surface through existing AgentEvent broadcast channel

## Self-Check: PASSED

- All 4 awareness module files exist
- Commit 1803e51 (Task 1) found
- Commit 1fb8925 (Task 2) found
- 32 awareness tests pass
- cargo check compiles without errors

---
*Phase: 02-awareness-and-judgment*
*Completed: 2026-03-27*
