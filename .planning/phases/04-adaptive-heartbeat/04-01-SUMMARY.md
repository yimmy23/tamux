---
phase: 04-adaptive-heartbeat
plan: 01
subsystem: agent
tags: [ema, heartbeat, priority-learning, operator-model, sqlite, serde]

# Dependency graph
requires:
  - phase: 02-core-heartbeat
    provides: "HeartbeatChecksConfig, heartbeat.rs orchestration, heartbeat_checks.rs, gateway_loop.rs scheduling"
  - phase: 03-transparent-autonomy
    provides: "action_audit table, AuditEntryRow, audit query/cleanup methods"
provides:
  - "Per-check priority_weight and priority_override fields on HeartbeatChecksConfig"
  - "EMA smoothing fields (ema_alpha, low_activity_frequency_factor, ema_activity_threshold) on AgentConfig"
  - "smoothed_activity_histogram on SessionRhythm for persisted EMA state"
  - "Pure functions: ema_update, smooth_activity_histogram, is_peak_activity_hour, should_run_check, compute_check_priority"
  - "AuditDismiss/AuditDismissResult protocol messages"
  - "user_action column on action_audit table with dismiss/count query methods"
affects: [04-02-PLAN, 04-03-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: [ema-smoothing, priority-weight-gating, dismissal-feedback-loop]

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/agent/operator_model.rs"
    - "crates/amux-daemon/src/agent/heartbeat.rs"
    - "crates/amux-daemon/src/history.rs"
    - "crates/amux-protocol/src/messages.rs"
    - "crates/amux-daemon/src/server.rs"

key-decisions:
  - "EMA alpha default 0.3 for ~5-session adaptation to new routines"
  - "Priority weight floor 0.1 per D-05: checks are never fully disabled"
  - "Decay rate 0.1 per dismissal, recovery rate 0.1 per acted-on recovery"
  - "AuditDismiss server handler stubbed (returns false) -- wired in Plan 02"

patterns-established:
  - "EMA smoothing: alpha * sample + (1 - alpha) * current for all activity adaptation"
  - "Priority weight gating: should_run_check(weight, cycle) skips checks via modular arithmetic"
  - "Composite feedback scoring: dismiss_penalty + inaction_penalty - recovery_bonus clamped to [0.1, 1.0]"

requirements-completed: [BEAT-06, BEAT-07, BEAT-09]

# Metrics
duration: 9min
completed: 2026-03-23
---

# Phase 4 Plan 1: Adaptive Heartbeat Foundation Summary

**Type contracts, EMA smoothing pure functions, priority weight gating, and dismissal tracking schema for activity-aware heartbeat scheduling**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-23T12:35:37Z
- **Completed:** 2026-03-23T12:45:02Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Extended HeartbeatChecksConfig with per-check priority_weight (default 1.0), priority_override, and reset_learned_priorities fields -- all backward-compatible with existing config.json
- Added ema_alpha (0.3), low_activity_frequency_factor (4), ema_activity_threshold (2.0) to AgentConfig with serde defaults
- Implemented EMA smoothing pure functions (ema_update, smooth_activity_histogram) with convergence-tested behavior
- Implemented priority computation pure functions (is_peak_activity_hour, should_run_check, compute_check_priority) with D-05 floor guarantee
- Added user_action column to action_audit table with dismiss/count/shown query methods on HistoryStore
- Added AuditDismiss/AuditDismissResult protocol messages with stub server handler

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend types, config, and protocol for adaptive heartbeat** - `c1b6762` (feat)
2. **Task 2: EMA smoothing, priority computation pure functions, dismissal tracking schema** - `892b91d` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/types.rs` - Priority weight, override, and EMA config fields on HeartbeatChecksConfig and AgentConfig
- `crates/amux-daemon/src/agent/operator_model.rs` - smoothed_activity_histogram on SessionRhythm, ema_update and smooth_activity_histogram pure functions
- `crates/amux-daemon/src/agent/heartbeat.rs` - is_peak_activity_hour, should_run_check, compute_check_priority pure functions
- `crates/amux-daemon/src/history.rs` - user_action column migration, dismiss_audit_entry, count_dismissals_by_type, count_shown_by_type, count_acted_on_by_type
- `crates/amux-protocol/src/messages.rs` - AuditDismiss ClientMessage variant, AuditDismissResult DaemonMessage variant
- `crates/amux-daemon/src/server.rs` - Stub handler for AuditDismiss message

## Decisions Made
- EMA alpha default 0.3 chosen for ~5-session adaptation window (per D-02 recommendation)
- Low activity frequency factor default 4 means heartbeat runs every 4th cycle during low-activity hours (per D-03)
- EMA activity threshold default 2.0 -- minimum smoothed count to consider an hour "active"
- Priority weight floor clamped at 0.1 (never fully disables a check, per D-05)
- AuditDismiss handler stubbed in server.rs to compile -- full wiring to HistoryStore deferred to Plan 02

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added stub AuditDismiss handler in server.rs**
- **Found during:** Task 1 (protocol message addition)
- **Issue:** Adding AuditDismiss to ClientMessage enum required an exhaustive match arm in server.rs to compile
- **Fix:** Added stub handler that logs and returns success=false with "not yet wired" message
- **Files modified:** crates/amux-daemon/src/server.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** c1b6762 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for compilation. Stub will be wired to HistoryStore in Plan 02. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All type contracts established for Plans 02 and 03
- Pure functions ready for integration into run_structured_heartbeat() (Plan 02)
- Dismissal tracking schema ready for server handler wiring (Plan 02)
- EMA smoothing functions ready for gateway_loop.rs frequency modulation (Plan 02)
- 602 daemon tests pass (16 new tests added in this plan)

---
*Phase: 04-adaptive-heartbeat*
*Completed: 2026-03-23*
