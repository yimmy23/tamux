---
phase: 05-memory-consolidation
plan: 02
subsystem: agent
tags: [consolidation, memory, decay, heuristics, patterns, tombstone, heartbeat, provenance]

# Dependency graph
requires:
  - phase: 05-memory-consolidation
    plan: 01
    provides: ConsolidationConfig, ConsolidationResult, SQLite tables, idle detection, decay functions, HeuristicStore/PatternStore fields
  - phase: 02-core-heartbeat
    provides: Heartbeat loop structure with phased execution
  - phase: 04-adaptive-heartbeat
    provides: Learned check weights (Phase 9), operator model integration
provides:
  - Fully functional consolidation tick with 3 sub-tasks (trace review, fact decay, tombstone cleanup)
  - Watermark-based trace review with heuristic promotion at configurable threshold
  - Memory fact decay with actual tombstoning of low-confidence facts below threshold
  - supersede_memory_fact with tombstone-before-update ordering and [SUPERSEDED] markers
  - Heartbeat Phase 10 consolidation integration with configurable budget
  - Provenance audit trail for all consolidation actions
affects: [05-03, 05-04]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Time-budgeted sub-task orchestration with deadline checking per iteration"
    - "Watermark-based incremental trace processing across daemon restarts"
    - "Tombstone-before-update crash-safe ordering for memory supersession"
    - "[SUPERSEDED] marker in memory files for audit traceability"
    - "Pattern-to-heuristic promotion pipeline at configurable occurrence threshold"

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/consolidation.rs
    - crates/amux-daemon/src/agent/memory.rs
    - crates/amux-daemon/src/agent/heartbeat.rs

key-decisions:
  - "HeuristicStore.update_tool uses succeeded:bool (not effectiveness:f64) matching existing API signature"
  - "Fact decay skips facts with no provenance record (cannot compute meaningful confidence without last_confirmed_at)"
  - "Duration per tool approximated as total trace duration / tool count for heuristic promotion"
  - "Duration_ms recomputed after Phase 10 so final heartbeat log includes consolidation time"

patterns-established:
  - "Consolidation sub-tasks check deadline at each iteration for clean budget enforcement"
  - "supersede_memory_fact as reusable tombstone+marker pattern for any memory target"

requirements-completed: [MEMO-01, MEMO-03, MEMO-04, MEMO-05, MEMO-06]

# Metrics
duration: 5min
completed: 2026-03-23
---

# Phase 5 Plan 2: Consolidation Engine Summary

**Time-budgeted consolidation tick with trace-to-heuristic promotion, exponential fact decay with tombstoning, and heartbeat Phase 10 integration**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-23T14:34:02Z
- **Completed:** 2026-03-23T14:39:02Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Replaced consolidation stub with 3-sub-task orchestrator: trace review, fact decay, tombstone cleanup -- all time-budgeted
- Trace review loads successful traces via watermark, records tool sequences in PatternStore, promotes to HeuristicStore when occurrences cross configurable threshold (default 3)
- Fact decay scans MEMORY.md, computes exponential confidence per fact, tombstones facts below configurable threshold (default 0.2) via supersede_memory_fact
- supersede_memory_fact writes tombstone to SQLite FIRST (crash-safe per Pitfall 2), then marks original with [SUPERSEDED] prefix in memory file per D-06
- Heartbeat Phase 10 calls consolidation with budget from ConsolidationConfig, logs results, includes consolidated flag in cycle log

## Task Commits

Each task was committed atomically:

1. **Task 1: Consolidation sub-tasks -- trace review, tombstone ops, decay with supersession, and tick orchestration** - `19ec7b9` (feat)
2. **Task 2: Wire consolidation into heartbeat Phase 10** - `3d1fa17` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/consolidation.rs` - Full consolidation engine: review_execution_traces, apply_fact_decay, cleanup_expired_tombstones, time-budgeted orchestration
- `crates/amux-daemon/src/agent/memory.rs` - supersede_memory_fact with [SUPERSEDED] marker and tombstone-before-update; MemoryFactCandidate and extract_memory_fact_candidates made pub(super)
- `crates/amux-daemon/src/agent/heartbeat.rs` - Phase 10 consolidation block with budget, logging, and consolidated flag

## Decisions Made
- Used `succeeded: bool` parameter for `HeuristicStore.update_tool` matching its existing API signature (plan pseudocode used `effectiveness: f64` which doesn't match)
- Facts with no provenance record are skipped during decay since confidence cannot be meaningfully computed without a last_confirmed_at timestamp
- Per-tool duration for heuristic promotion approximated as total trace duration divided by tool count
- Recomputed duration_ms after Phase 10 so the final heartbeat log accurately includes consolidation processing time

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Made MemoryFactCandidate and extract_memory_fact_candidates pub(super)**
- **Found during:** Task 1 (consolidation engine implementation)
- **Issue:** extract_memory_fact_candidates and MemoryFactCandidate were private in memory.rs, inaccessible from consolidation.rs
- **Fix:** Changed both to pub(super) visibility, along with MemoryFactCandidate's fields
- **Files modified:** crates/amux-daemon/src/agent/memory.rs
- **Verification:** cargo build -p tamux-daemon compiles cleanly
- **Committed in:** 19ec7b9 (Task 1 commit)

**2. [Rule 1 - Bug] Adapted HeuristicStore.update_tool call signature**
- **Found during:** Task 1 (trace review implementation)
- **Issue:** Plan pseudocode called update_tool with (tool_name, task_type, effectiveness: f64, duration_ms: u64) but actual signature uses (tool_name, task_type, succeeded: bool, duration_ms: u64)
- **Fix:** Used succeeded=true (traces are already filtered to successful outcomes) and approximated per-tool duration
- **Files modified:** crates/amux-daemon/src/agent/consolidation.rs
- **Verification:** cargo build -p tamux-daemon compiles cleanly
- **Committed in:** 19ec7b9 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes required for compilation and API correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Consolidation engine fully operational, ready for Plan 03 (cross-session continuity: HeuristicStore/PatternStore persistence)
- Plan 04 (memory refinement) can use the same consolidation tick framework to add Sub-task 4
- supersede_memory_fact available as a reusable primitive for any future memory management

## Self-Check: PASSED

- All 3 modified files exist on disk
- Both task commits (19ec7b9, 3d1fa17) verified in git log
- 12 consolidation tests pass
- Build compiles cleanly (no new warnings)

---
*Phase: 05-memory-consolidation*
*Completed: 2026-03-23*
