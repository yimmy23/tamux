---
phase: 02-core-heartbeat
plan: 01
subsystem: agent
tags: [heartbeat, rust, serde, croner, cron, types, agent-engine]

# Dependency graph
requires:
  - phase: 01-production-foundation
    provides: "Circuit breaker registry, shared HistoryStore, WAL mode SQLite"
provides:
  - "HeartbeatChecksConfig, HeartbeatCheckType, CheckSeverity, CheckDetail, HeartbeatCheckResult, HeartbeatDigestItem types"
  - "AgentConfig.heartbeat_cron, heartbeat_checks, quiet_hours_start/end, dnd_enabled fields"
  - "AgentEvent::HeartbeatDigest variant forwarded by server.rs"
  - "interval_mins_to_cron helper for legacy config migration"
  - "Four built-in check functions: check_stale_todos, check_stuck_goal_runs, check_unreplied_messages, check_repo_changes"
  - "croner crate dependency for cron scheduling"
affects: [02-core-heartbeat plan 02 scheduling, 02-core-heartbeat plan 03 synthesis wiring]

# Tech tracking
tech-stack:
  added: [croner 3.x]
  patterns: [structured heartbeat checks returning HeartbeatCheckResult, severity escalation based on threshold multiples, spawn_blocking for git CLI calls]

key-files:
  created:
    - "crates/amux-daemon/src/agent/heartbeat_checks.rs"
  modified:
    - "crates/amux-daemon/Cargo.toml"
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/agent/mod.rs"
    - "crates/amux-daemon/src/server.rs"

key-decisions:
  - "Async test helper constructs full AgentEngine with real HistoryStore rather than mocking -- ensures integration correctness"
  - "check_unreplied_messages returns items_found=0 in Phase 2 to avoid false positives -- full unreplied detection deferred to Phase 8"
  - "Severity escalation uses threshold multipliers (1.5x/3x for todos, 2x/4x for goals) for progressive urgency"

patterns-established:
  - "make_test_engine async helper: constructs minimal AgentEngine for testing check functions without external dependencies"
  - "HeartbeatCheckResult as universal return type: check_type + items_found + summary + details for all checks"
  - "spawn_blocking wrapper for blocking git CLI calls in async context"

requirements-completed: [BEAT-01, BEAT-02, BEAT-04, BEAT-05]

# Metrics
duration: 9min
completed: 2026-03-23
---

# Phase 02 Plan 01: Heartbeat Types and Check Functions Summary

**Structured heartbeat type contracts (config, checks, digest, events) with four built-in data-gathering check functions using croner cron scheduling**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-23T07:37:04Z
- **Completed:** 2026-03-23T07:46:36Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Defined all heartbeat type contracts (HeartbeatChecksConfig, HeartbeatCheckResult, HeartbeatCheckType, CheckDetail, CheckSeverity, HeartbeatDigestItem) with serde defaults and backward-compatible AgentConfig fields
- Added AgentEvent::HeartbeatDigest variant with server.rs forwarding for structured check results
- Implemented four built-in check functions (stale todos, stuck goal runs, unreplied messages, repo changes) with severity escalation and graceful error handling
- Added interval_mins_to_cron helper for legacy heartbeat interval migration to cron expressions
- 15 new tests (8 type contract tests + 7 check function integration tests) with zero regressions across 520 total daemon tests

## Task Commits

Each task was committed atomically:

1. **Task 1: Add croner dependency and define heartbeat type contracts** - `5bc3aa9` (test: RED) + `8658e8f` (feat: GREEN)
2. **Task 2: Implement four built-in heartbeat check functions** - `a93d276` (feat: implementation + tests)

## Files Created/Modified
- `crates/amux-daemon/Cargo.toml` - Added croner dependency
- `crates/amux-daemon/src/agent/types.rs` - HeartbeatChecksConfig, HeartbeatCheckType, CheckSeverity, CheckDetail, HeartbeatCheckResult, HeartbeatDigestItem types; AgentConfig fields (heartbeat_cron, heartbeat_checks, quiet_hours_start/end, dnd_enabled); AgentEvent::HeartbeatDigest variant; interval_mins_to_cron helper; 8 unit tests
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Four check functions (check_stale_todos, check_stuck_goal_runs, check_unreplied_messages, check_repo_changes) with 7 integration tests
- `crates/amux-daemon/src/agent/mod.rs` - Registered heartbeat_checks module
- `crates/amux-daemon/src/server.rs` - Forward HeartbeatDigest events to clients

## Decisions Made
- Used async test helper that constructs real AgentEngine with HistoryStore rather than mocking -- this ensures the check functions integrate correctly with the actual data structures
- check_unreplied_messages returns items_found=0 conservatively in Phase 2 to avoid false positives -- true unreplied message detection requires per-message response tracking which is deferred to Phase 8
- Severity escalation uses configurable threshold multipliers (Low at 1x, Medium at 1.5x, High at 3x for todos; Medium at 1x, High at 2x, Critical at 4x for goals) providing progressive urgency without hardcoded thresholds

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed block_on panic in async test context**
- **Found during:** Task 2 (check function tests)
- **Issue:** Test helper used `tokio::runtime::Handle::current().block_on()` inside `#[tokio::test]` causing "Cannot start a runtime from within a runtime" panic
- **Fix:** Changed `make_test_engine` from sync to async function, using `.await` for HistoryStore construction
- **Files modified:** `crates/amux-daemon/src/agent/heartbeat_checks.rs`
- **Verification:** All 7 heartbeat_checks tests pass
- **Committed in:** a93d276

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Essential fix for test infrastructure. No scope creep.

## Issues Encountered
None beyond the auto-fixed test helper issue above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
- `check_unreplied_messages` returns `items_found: 0` always -- intentional Phase 2 stub. Full unreplied message detection deferred to Phase 8 (gateway response tracking).

## Next Phase Readiness
- All heartbeat types and check functions are ready for Plan 02 (scheduling/persistence) to wire into the heartbeat loop
- HeartbeatChecksConfig provides per-check enable/disable and cron overrides for Plan 02's cron scheduler
- HeartbeatDigest event variant ready for Plan 03's LLM synthesis to emit structured digest results
- croner dependency available for Plan 02's cron-based scheduling

## Self-Check: PASSED

All files exist, all commits verified.

---
*Phase: 02-core-heartbeat*
*Completed: 2026-03-23*
