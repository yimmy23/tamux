---
phase: 05-memory-consolidation
plan: 01
subsystem: agent
tags: [consolidation, memory, decay, sqlite, heuristics, patterns, idle-detection]

# Dependency graph
requires:
  - phase: 01-production-foundation
    provides: HistoryStore with SQLite WAL, tokio-rusqlite async closures
  - phase: 02-core-heartbeat
    provides: Heartbeat loop structure, AgentEngine fields pattern
  - phase: 04-adaptive-heartbeat
    provides: Learned check weights, operator model integration
provides:
  - ConsolidationConfig on AgentConfig with all tunable parameters
  - ConsolidationResult struct for tick outcome reporting
  - memory_tombstones and consolidation_state SQLite tables
  - CRUD methods for tombstones (insert/list/delete/restore) and state (get/set)
  - list_recent_successful_traces watermark-based query for execution traces
  - is_idle_for_consolidation pure function (4-signal check)
  - compute_decay_confidence exponential decay function (69h half-life)
  - HeuristicStore and PatternStore as RwLock fields on AgentEngine
  - maybe_run_consolidation_if_idle stub entry point (Plan 02 fills)
affects: [05-02, 05-03, 05-04]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Exponential decay with configurable half-life for memory confidence"
    - "4-signal idle detection (tasks + goals + streams + presence)"
    - "Tombstone pattern for recoverable memory supersession"
    - "Watermark-based trace retrieval for incremental processing"

key-files:
  created:
    - crates/amux-daemon/src/agent/consolidation.rs
  modified:
    - crates/amux-daemon/src/agent/types.rs
    - crates/amux-daemon/src/agent/engine.rs
    - crates/amux-daemon/src/agent/mod.rs
    - crates/amux-daemon/src/history.rs
    - crates/amux-daemon/src/agent/heartbeat_checks.rs

key-decisions:
  - "TaskStatus::InProgress used for active task detection (not a separate AgentTaskStatus)"
  - "Tombstone replaced_at set to created_at on insert (single timestamp for creation)"
  - "ExecutionTraceRow uses Option<T> for all nullable columns for robustness"

patterns-established:
  - "Consolidation pure functions testable without AgentEngine construction"
  - "Tombstone CRUD follows established tokio-rusqlite .call() pattern with String cloning"

requirements-completed: [MEMO-02, MEMO-07]

# Metrics
duration: 12min
completed: 2026-03-23
---

# Phase 5 Plan 1: Consolidation Foundation Summary

**Memory consolidation types, SQLite tables, idle detection, and exponential decay functions with 21 passing tests**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-23T14:18:11Z
- **Completed:** 2026-03-23T14:30:11Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- ConsolidationConfig with all 8 tunable fields (budget, idle threshold, tombstone TTL, half-life, promotion threshold, auto-resume, supersede threshold) added to AgentConfig with serde defaults
- memory_tombstones and consolidation_state SQLite tables with CRUD methods and indexes
- Pure is_idle_for_consolidation function checking 4 simultaneous signals (no tasks, no goals, no streams, operator idle)
- Pure compute_decay_confidence exponential decay function with 69-hour default half-life
- HeuristicStore and PatternStore as persistent learning fields on AgentEngine
- Stub entry point maybe_run_consolidation_if_idle ready for Plan 02 wiring

## Task Commits

Each task was committed atomically:

1. **Task 1: Consolidation types, config, SQLite tables, and AgentEngine learning store fields** - `ed8f999` (feat)
2. **Task 2: Consolidation module with idle detection and decay pure functions** - `d534faa` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/consolidation.rs` - New module: idle detection, decay math, entry point stub, 12 unit tests
- `crates/amux-daemon/src/agent/types.rs` - ConsolidationConfig, ConsolidationResult structs, field on AgentConfig, 4 unit tests
- `crates/amux-daemon/src/agent/engine.rs` - heuristic_store and pattern_store RwLock fields on AgentEngine
- `crates/amux-daemon/src/agent/mod.rs` - Module registration for consolidation
- `crates/amux-daemon/src/history.rs` - SQLite tables, MemoryTombstoneRow/ExecutionTraceRow structs, 7 CRUD methods, 5 unit tests
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Test helper updated with new AgentEngine fields

## Decisions Made
- Used `TaskStatus::InProgress` (the actual enum variant) rather than `AgentTaskStatus::InProgress` from the plan's pseudocode
- Tombstone `replaced_at` is set to `created_at` on initial insert since the replacement event timestamp equals creation
- ExecutionTraceRow fields use `Option<T>` for all nullable columns to prevent query failures on NULL data
- Added `#[allow(dead_code)]` on constants used only in tests until Plan 02 wires them

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed missing consolidation field in AgentConfig Default impl**
- **Found during:** Task 1 (compilation check)
- **Issue:** AgentConfig has a manual Default impl that must list every field; adding `consolidation` to the struct without updating Default caused E0063
- **Fix:** Added `consolidation: ConsolidationConfig::default()` to the Default impl
- **Files modified:** crates/amux-daemon/src/agent/types.rs
- **Verification:** `cargo build -p tamux-daemon` compiles cleanly
- **Committed in:** ed8f999 (Task 1 commit)

**2. [Rule 3 - Blocking] Fixed missing fields in heartbeat_checks test helper**
- **Found during:** Task 1 (test compilation)
- **Issue:** heartbeat_checks.rs constructs AgentEngine directly in a test helper and was missing the new `heuristic_store` and `pattern_store` fields
- **Fix:** Added the two new fields with default values to the test helper constructor
- **Files modified:** crates/amux-daemon/src/agent/heartbeat_checks.rs
- **Verification:** `cargo test -p tamux-daemon -- consolidation` passes
- **Committed in:** ed8f999 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes were required compilation fixes from adding new struct fields. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All types, tables, and pure functions ready for Plan 02 (consolidation engine)
- maybe_run_consolidation_if_idle stub ready to be filled with trace review, fact decay, and heuristic promotion logic
- HeuristicStore/PatternStore fields ready for persistence loading in Plan 03 (cross-session continuity)

## Self-Check: PASSED

- All 6 created/modified files exist on disk
- Both task commits (ed8f999, d534faa) verified in git log
- 21 tests pass: 12 consolidation, 4 type, 5 history

---
*Phase: 05-memory-consolidation*
*Completed: 2026-03-23*
