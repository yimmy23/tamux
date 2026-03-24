---
phase: 03-transparent-autonomy
plan: 02
subsystem: agent-runtime
tags: [audit, explanation, heartbeat, escalation, causal-traces, confidence, sqlite]

# Dependency graph
requires:
  - phase: 03-transparent-autonomy-01
    provides: "explanation.rs module, AuditAction/EscalationUpdate events, AuditConfig, action_audit table, AuditEntryRow, AuditQuery IPC"
provides:
  - "Heartbeat cycle creates audit entries with template explanations for each actionable digest item"
  - "HeartbeatDigest events carry populated explanation field per D-01"
  - "Causal trace persistence (skill/tool) creates audit entries with confidence bands"
  - "Escalation module provides escalation_audit_data() helper and EscalationLevel::as_label()"
  - "Audit cleanup runs on each heartbeat cycle via cleanup_action_audit"
affects: [03-03-tui-audit, 03-04-electron-audit, 03-05-cli-audit]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Heartbeat audit entries created per-item with scope guard (config.audit.scope.heartbeat)"
    - "Causal trace audit entries carry confidence values from estimated_success_prob"
    - "Escalation audit data helper produces all fields for AgentEngine callers to persist/broadcast"
    - "Composite explanation built from multiple digest items: single item uses template directly, multiple joined with count prefix"

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/agent/heartbeat.rs"
    - "crates/amux-daemon/src/agent/causal_traces.rs"
    - "crates/amux-daemon/src/agent/metacognitive/escalation.rs"

key-decisions:
  - "HeartbeatDigest confidence set to None (heartbeat checks don't have probability; confidence applies to causal trace entries)"
  - "Composite explanation joins item explanations with count prefix for multi-item digests"
  - "Escalation audit helper is a pure function in escalation.rs; AgentEngine callers do the actual persist/broadcast"
  - "Audit cleanup piggybacks on heartbeat cycle (runs after audit entry creation)"

patterns-established:
  - "Scope-guarded audit entry creation: check config.audit.scope.{type} before creating entries"
  - "Non-breaking audit: all insert_action_audit calls wrapped in if-let-Err with tracing::warn"
  - "EscalationAuditData struct as intermediary between pure escalation logic and AgentEngine I/O"

requirements-completed: [TRNS-01, TRNS-02, TRNS-04, TRNS-05]

# Metrics
duration: 5min
completed: 2026-03-23
---

# Phase 03 Plan 02: Daemon Integration Summary

**Wired explanation generation and audit entries into heartbeat cycle, causal trace persistence, and escalation module with scope-guarded creation and confidence bands**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-23T10:39:27Z
- **Completed:** 2026-03-23T10:45:26Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Populated HeartbeatDigest explanation field with composite template-generated explanations per D-01 (single item uses template directly, multi-item joins with count prefix)
- Created audit entries for each actionable heartbeat digest item with AuditAction broadcast and cleanup_action_audit on every heartbeat cycle
- Wired audit entry creation into persist_skill_selection_causal_trace and persist_tool_selection_causal_trace with confidence values and confidence bands
- Added EscalationLevel::as_label() ("L0"/"L1"/"L2"/"L3") and escalation_audit_data() helper producing EscalationAuditData for AgentEngine callers
- All audit creation respects AuditConfig.scope toggles; audit failures logged as warnings, never break primary functionality

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire audit entry creation and HeartbeatDigest enrichment into heartbeat cycle and causal trace persistence** - `c26cc2b` (feat)
2. **Task 2: Wire escalation level transitions into audit trail and EscalationUpdate broadcasts** - `638f919` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/heartbeat.rs` - HeartbeatDigest explanation population, per-item audit entries with AuditAction broadcast, cleanup_action_audit on heartbeat cycle, check_type_to_action_type helper
- `crates/amux-daemon/src/agent/causal_traces.rs` - Audit entry creation in persist_skill_selection_causal_trace and persist_tool_selection_causal_trace with confidence bands
- `crates/amux-daemon/src/agent/metacognitive/escalation.rs` - EscalationLevel::as_label(), EscalationAuditData struct, escalation_audit_data() helper, 4 new tests

## Decisions Made
- HeartbeatDigest confidence is always None because heartbeat checks don't carry probability -- confidence applies to individual causal trace audit entries
- Composite explanation for multi-item digests uses "Found N items: ..." prefix; single item uses template directly
- Escalation audit helper is a pure function producing data, not performing I/O -- matches the module's pattern of being a data-only module without access to HistoryStore/event_tx
- Audit cleanup runs after heartbeat audit entry creation (not before) to include the entries just created in retention calculations

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All audit entry creation is wired into heartbeat, causal traces, and escalation data helpers
- Plan 03 (TUI) can consume AuditAction events and query via AuditQuery
- Plan 04 (Electron) can render AuditAction events and HeartbeatDigest.explanation in the audit panel
- Plan 05 (CLI) can implement `tamux audit` using AuditQuery IPC
- Escalation audit data helper is ready for AgentEngine integration when escalation transitions are fully wired into the agent loop

## Self-Check: PASSED

- All 3 modified files exist on disk
- Both task commits (c26cc2b, 638f919) verified in git log
- All acceptance criteria pass (grep checks)
- 578 daemon tests pass with 0 failures (0 regressions)
- Full workspace compilation succeeds (51 pre-existing warnings only)

---
*Phase: 03-transparent-autonomy*
*Completed: 2026-03-23*
