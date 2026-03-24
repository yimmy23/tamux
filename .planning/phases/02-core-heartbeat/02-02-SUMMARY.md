---
phase: 02-core-heartbeat
plan: 02
subsystem: agent
tags: [heartbeat, rust, croner, cron, chrono, sqlite, scheduling, quiet-hours, dnd]

# Dependency graph
requires:
  - phase: 02-core-heartbeat plan 01
    provides: "HeartbeatChecksConfig, interval_mins_to_cron, AgentConfig heartbeat fields, croner dependency"
  - phase: 01-production-foundation
    provides: "Circuit breaker registry, shared HistoryStore, WAL mode SQLite"
provides:
  - "Cron-based heartbeat scheduling in gateway_loop.rs (replaces fixed interval)"
  - "check_quiet_window pure function and is_quiet_hours async method for DND/quiet hours suppression"
  - "resolve_cron_from_config and resolve_heartbeat_cron for cron expression resolution"
  - "config_notify (tokio::sync::Notify) for heartbeat schedule hot-reload"
  - "heartbeat_history SQLite table with insert and query methods"
  - "HeartbeatHistoryRow struct for typed history query results"
affects: [02-core-heartbeat plan 03 synthesis wiring, phase 04 adaptive heartbeat]

# Tech tracking
tech-stack:
  added: [chrono 0.4]
  patterns: [cron-based tokio::select scheduling via croner, pure function extraction for testable time logic, config change notification via tokio::sync::Notify]

key-files:
  created: []
  modified:
    - "crates/amux-daemon/Cargo.toml"
    - "crates/amux-daemon/src/agent/heartbeat.rs"
    - "crates/amux-daemon/src/agent/gateway_loop.rs"
    - "crates/amux-daemon/src/agent/engine.rs"
    - "crates/amux-daemon/src/agent/config.rs"
    - "crates/amux-daemon/src/agent/heartbeat_checks.rs"
    - "crates/amux-daemon/src/history.rs"

key-decisions:
  - "Pure function check_quiet_window extracted for testability instead of testing async is_quiet_hours directly"
  - "tokio::sync::Notify used for config change notification rather than channel-based approach -- simpler, no capacity concerns"
  - "chrono added as explicit dependency (was only transitive via croner) for Timelike::hour() access"
  - "Heartbeat schedule recomputation triggered by config_notify in gateway_loop select arm"

patterns-established:
  - "check_quiet_window pure function: extract time-dependent logic into pure functions for deterministic testing"
  - "config_notify pattern: use tokio::sync::Notify on AgentEngine to signal config changes to background loops"
  - "Cron scheduling pattern: parse cron expression, compute next occurrence, sleep_until, recompute after execution"

requirements-completed: [BEAT-01, BEAT-03, BEAT-05]

# Metrics
duration: 11min
completed: 2026-03-23
---

# Phase 02 Plan 02: Heartbeat Scheduling and Persistence Summary

**Cron-based heartbeat scheduling with quiet hours suppression, config hot-reload, and SQLite heartbeat_history persistence layer**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-23T07:50:22Z
- **Completed:** 2026-03-23T08:02:09Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Replaced fixed tokio::time::interval heartbeat with croner cron-based scheduling in gateway_loop.rs
- Implemented quiet hours and DND suppression with midnight-wrap handling (e.g., 22:00-05:59)
- Added config hot-reload via tokio::sync::Notify so heartbeat schedule updates without daemon restart
- Created heartbeat_history SQLite table with insert/query methods and HeartbeatHistoryRow struct
- 15 new unit tests for quiet hours logic and cron resolution, 535 total tests pass with zero regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Cron scheduling and quiet hours** - `5a41c59` (test: pure helpers + tests) + `04901b6` (feat: gateway_loop cron scheduling)
2. **Task 2: SQLite heartbeat_history table and persistence** - `a528e24` (feat: table + methods)

## Files Created/Modified
- `crates/amux-daemon/Cargo.toml` - Added chrono 0.4 dependency
- `crates/amux-daemon/src/agent/heartbeat.rs` - check_quiet_window pure function, resolve_cron_from_config, is_quiet_hours/resolve_heartbeat_cron async methods, 15 unit tests
- `crates/amux-daemon/src/agent/gateway_loop.rs` - Replaced heartbeat_tick interval with cron-based sleep_until + quiet hours suppression + config_notify listener
- `crates/amux-daemon/src/agent/engine.rs` - Added config_notify: tokio::sync::Notify field
- `crates/amux-daemon/src/agent/config.rs` - Signal config_notify on set_config and set_config_item_json
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Added config_notify field to test helper AgentEngine constructor
- `crates/amux-daemon/src/history.rs` - heartbeat_history table creation, HeartbeatHistoryRow struct, insert_heartbeat_history and list_heartbeat_history methods

## Decisions Made
- Extracted `check_quiet_window` as a pure function taking explicit hour/config values for deterministic unit testing, rather than testing the async `is_quiet_hours` which depends on `chrono::Local::now()`
- Used `tokio::sync::Notify` for config change notification to the run_loop -- simpler than adding a channel, no capacity management needed
- Added chrono as an explicit dependency even though it was available transitively via croner -- following best practice of declaring all direct dependencies

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Moved agent_backend extraction after cron resolution to fix partial move**
- **Found during:** Task 1 (gateway_loop.rs cron scheduling)
- **Issue:** `config.agent_backend` was moved before `resolve_cron_from_config(&config)`, causing borrow-after-partial-move error since AgentBackend does not implement Copy
- **Fix:** Reordered lines so cron resolution happens before agent_backend extraction
- **Files modified:** `crates/amux-daemon/src/agent/gateway_loop.rs`
- **Verification:** `cargo check -p tamux-daemon` compiles successfully
- **Committed in:** 04901b6

**2. [Rule 3 - Blocking] Added config_notify field to test helper AgentEngine constructor**
- **Found during:** Task 1 (test compilation)
- **Issue:** `heartbeat_checks.rs` test helper manually constructs AgentEngine and was missing the new `config_notify` field
- **Fix:** Added `config_notify: tokio::sync::Notify::new()` to the test helper
- **Files modified:** `crates/amux-daemon/src/agent/heartbeat_checks.rs`
- **Verification:** All 535 tests pass
- **Committed in:** 04901b6

**3. [Rule 2 - Missing Critical] Added config_notify for heartbeat schedule hot-reload**
- **Found during:** Task 1 (config hot-reload design)
- **Issue:** Plan suggested adding cron recomputation to watcher_refresh_rx handler, but that handler is for repo file watches, not config changes. No config change notification mechanism existed.
- **Fix:** Added `config_notify: tokio::sync::Notify` to AgentEngine, signaled from `set_config()` and `set_config_item_json()`, listened in gateway_loop select arm
- **Files modified:** `crates/amux-daemon/src/agent/engine.rs`, `crates/amux-daemon/src/agent/config.rs`, `crates/amux-daemon/src/agent/gateway_loop.rs`
- **Verification:** Config changes trigger heartbeat schedule recomputation via the Notify mechanism
- **Committed in:** 04901b6

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing critical)
**Impact on plan:** All fixes necessary for correctness and compilation. The config_notify approach is cleaner than the plan's suggested watcher_refresh_rx approach. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all planned functionality is fully implemented.

## Next Phase Readiness
- Cron scheduling and quiet hours suppression ready for Plan 03 to wire structured heartbeat synthesis
- heartbeat_history table ready for Plan 03 to persist cycle results after LLM synthesis
- config_notify mechanism available for any future background loop features needing config change awareness
- resolve_cron_from_config and check_quiet_window helpers can be reused by Plan 03's synthesis flow

## Self-Check: PASSED

All files exist, all commits verified.

---
*Phase: 02-core-heartbeat*
*Completed: 2026-03-23*
