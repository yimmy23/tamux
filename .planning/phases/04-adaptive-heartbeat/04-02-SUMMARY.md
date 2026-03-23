---
phase: 04-adaptive-heartbeat
plan: 02
subsystem: agent
tags: [heartbeat, adaptive-scheduling, priority-gating, ema, feedback-loop, operator-model]

# Dependency graph
requires:
  - phase: 04-adaptive-heartbeat/plan-01
    provides: "Pure functions (is_peak_activity_hour, should_run_check, compute_check_priority, smooth_activity_histogram), priority weight/override config fields, AuditDismiss protocol messages, dismiss/count query methods on HistoryStore"
  - phase: 02-core-heartbeat
    provides: "run_structured_heartbeat orchestration, gateway_loop.rs cron scheduling, HeartbeatChecksConfig, heartbeat_checks.rs"
  - phase: 03-transparent-autonomy
    provides: "action_audit table, audit entries, AuditQuery handler pattern"
provides:
  - "Activity-aware heartbeat scheduling with cold start protection in gateway_loop.rs"
  - "Priority-gated check execution with three-level cascade (override > learned > config)"
  - "Learned weight feedback loop: dismissals/inaction/acted-on signals update check weights after each heartbeat cycle"
  - "AuditDismiss IPC handler wired to HistoryStore.dismiss_audit_entry"
  - "EMA smoothed histogram update after each heartbeat cycle"
  - "run_structured_heartbeat_adaptive entry point with backward-compatible wrapper"
affects: [04-03-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: [activity-aware-scheduling, three-level-priority-cascade, feedback-loop-learning]

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/agent/gateway_loop.rs"
    - "crates/amux-daemon/src/agent/heartbeat.rs"
    - "crates/amux-daemon/src/agent/engine.rs"
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/server.rs"
    - "crates/amux-daemon/src/agent/heartbeat_checks.rs"

key-decisions:
  - "Action type keys use actual DB values (stale_todo, stuck_goal, etc.) not plan-specified heartbeat_ prefix"
  - "Decay rate 0.05 per dismissal (5%) and recovery rate 0.1 per acted-on (10%) for gradual weight adaptation"
  - "Seven-day lookback window for feedback signal aggregation"
  - "EMA smoothing runs after persistence to avoid blocking the heartbeat critical path"

patterns-established:
  - "Three-level priority cascade: config override > learned weight > config default for all gating decisions"
  - "Feedback loop pattern: query DB signals -> compute weights -> store on engine -> gate next cycle"
  - "Cold start protection: session_count < 5 bypasses adaptive logic"

requirements-completed: [BEAT-06, BEAT-07]

# Metrics
duration: 8min
completed: 2026-03-23
---

# Phase 4 Plan 2: Adaptive Heartbeat Wiring Summary

**Activity-aware heartbeat scheduling with priority-gated checks, learned weight feedback loop, and AuditDismiss IPC wiring**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-23T12:47:14Z
- **Completed:** 2026-03-23T12:55:30Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Wired activity-aware scheduling into gateway_loop.rs: heartbeat runs at full frequency during peak hours, skips cycles during low-activity periods (per D-03 with configurable skip factor)
- Cold start protection ensures adaptive scheduling is disabled for first 5 sessions, treating all hours as peak
- Replaced simple enabled-boolean check gating with three-level priority cascade: config override (highest) > learned weight > config default (lowest)
- Closed the D-04 feedback loop: after each heartbeat cycle, queries 7-day dismissal/inaction/acted-on signals from HistoryStore, computes updated weights via compute_check_priority, stores in learned_check_weights on AgentEngine
- Wired AuditDismiss server handler to HistoryStore.dismiss_audit_entry (replacing Plan 01 stub)
- EMA smoothed activity histogram updated at end of each heartbeat cycle for persistence across restarts

## Task Commits

Each task was committed atomically:

1. **Task 1: Activity-aware scheduling in gateway_loop + EMA update wiring** - `cf622c6` (feat)
2. **Task 2: Priority gating with learned weight update loop + AuditDismiss server handler** - `a9e4850` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/gateway_loop.rs` - heartbeat_cycle_count counter, activity-aware scheduling with cold start protection, is_peak_activity_hour import
- `crates/amux-daemon/src/agent/heartbeat.rs` - run_structured_heartbeat_adaptive with priority gating, EMA update, feedback loop, reset_learned_priorities check
- `crates/amux-daemon/src/agent/engine.rs` - learned_check_weights HashMap field on AgentEngine
- `crates/amux-daemon/src/agent/types.rs` - Added Hash derive to HeartbeatCheckType for HashMap key usage
- `crates/amux-daemon/src/server.rs` - AuditDismiss handler wired to HistoryStore.dismiss_audit_entry
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Added learned_check_weights to test helper AgentEngine constructor

## Decisions Made
- Used actual action_type DB values (`stale_todo`, `stuck_goal`, `unreplied_message`, `repo_change`) for feedback signal queries instead of plan-specified `heartbeat_` prefix (the plan had incorrect keys; using actual values ensures the feedback loop matches the audit entries created by the heartbeat)
- Decay rate 0.05 per dismissal and recovery rate 0.1 per acted-on chosen for gradual adaptation (plan suggested 0.05/0.1 as Claude's discretion)
- Seven-day lookback window for feedback aggregation provides sufficient signal while limiting stale history influence
- Fixed Send trait issue in tracing::debug by reading learned_check_weights into a local snapshot before logging

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected action_type key strings for feedback queries**
- **Found during:** Task 2 (weight update loop implementation)
- **Issue:** Plan specified action_type keys as `heartbeat_stale_todos`, `heartbeat_stuck_goals`, etc. but the actual DB values are `stale_todo`, `stuck_goal`, `unreplied_message`, `repo_change`
- **Fix:** Used actual DB values from check_type_to_action_type mapping
- **Files modified:** crates/amux-daemon/src/agent/heartbeat.rs
- **Verification:** Matches check_type_to_action_type and audit entry creation code
- **Committed in:** a9e4850 (Task 2 commit)

**2. [Rule 3 - Blocking] Added Hash derive to HeartbeatCheckType**
- **Found during:** Task 2 (learned_check_weights HashMap creation)
- **Issue:** HeartbeatCheckType only had PartialEq + Eq derives, missing Hash needed for HashMap key
- **Fix:** Added Hash to derive list
- **Files modified:** crates/amux-daemon/src/agent/types.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** a9e4850 (Task 2 commit)

**3. [Rule 3 - Blocking] Added learned_check_weights to test helper constructor**
- **Found during:** Task 2 (test compilation)
- **Issue:** heartbeat_checks.rs test helper creates AgentEngine directly; missing new field caused compilation failure
- **Fix:** Added `learned_check_weights: RwLock::new(HashMap::new())` to test helper
- **Files modified:** crates/amux-daemon/src/agent/heartbeat_checks.rs
- **Verification:** All 602 daemon tests pass
- **Committed in:** a9e4850 (Task 2 commit)

**4. [Rule 3 - Blocking] Fixed Send trait violation in tracing::debug**
- **Found during:** Task 2 (weight update loop)
- **Issue:** `tracing::debug!(weights = ?self.learned_check_weights.read().await, ...)` holds format args across an await point, violating Send
- **Fix:** Read weights into a local clone before logging
- **Files modified:** crates/amux-daemon/src/agent/heartbeat.rs
- **Verification:** cargo check -p tamux-daemon passes
- **Committed in:** a9e4850 (Task 2 commit)

---

**Total deviations:** 4 auto-fixed (1 bug, 3 blocking)
**Impact on plan:** All auto-fixes necessary for correctness and compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All activity-aware scheduling and priority gating wired and operational
- Learned weights persist on AgentEngine across heartbeat cycles (reset on daemon restart; persistent storage is a potential future enhancement)
- AuditDismiss fully wired: user feedback flows from client -> server -> HistoryStore -> feedback loop -> weight update
- Plan 03 (M2 anticipatory integration) can proceed: heartbeat orchestration is fully adaptive
- 602 daemon tests pass with zero regressions

## Self-Check: PASSED

- All 6 modified files exist on disk
- Commit cf622c6 (Task 1) found in git log
- Commit a9e4850 (Task 2) found in git log
- SUMMARY.md created at expected path

---
*Phase: 04-adaptive-heartbeat*
*Completed: 2026-03-23*
