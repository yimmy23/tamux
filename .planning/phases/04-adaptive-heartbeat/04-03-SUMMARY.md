---
phase: 04-adaptive-heartbeat
plan: 03
subsystem: agent
tags: [anticipatory, heartbeat, learning-transparency, dismiss, explanation, morning-brief]

# Dependency graph
requires:
  - phase: 04-adaptive-heartbeat/plan-01
    provides: "EMA smoothing, priority weights, AuditDismiss protocol messages, HeartbeatCheckType with Hash"
  - phase: 04-adaptive-heartbeat/plan-02
    provides: "run_structured_heartbeat_adaptive, learned_check_weights, priority gating, AuditDismiss server handler"
  - phase: 03-transparent-autonomy
    provides: "action_audit table, AuditEntryRow, audit feed UI in TUI and Electron"
provides:
  - "Anticipatory items (morning brief, stuck hints, hydration) merged into heartbeat LLM synthesis prompt"
  - "Morning brief augments only first heartbeat of session, clears after successful synthesis"
  - "Learning transparency templates: schedule_learned, check_deprioritized, check_reprioritized"
  - "Pattern-change detection: peak hours drift >2 hours, learned weight crossing below 0.5"
  - "Learning observations appended to synthesis prompt as dedicated section"
  - "TUI 'd' keybinding to dismiss audit entries with optimistic update"
  - "Electron dismiss button on AuditRow with IPC bridge to daemon"
  - "dismissAuditEntry exposed on Electron contextBridge"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: [anticipatory-heartbeat-merge, learning-transparency-invocation, optimistic-dismiss]

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/agent/heartbeat.rs"
    - "crates/amux-daemon/src/agent/anticipatory.rs"
    - "crates/amux-daemon/src/agent/explanation.rs"
    - "crates/amux-tui/src/state/audit.rs"
    - "crates/amux-tui/src/widgets/audit_feed.rs"
    - "crates/amux-tui/src/state/mod.rs"
    - "crates/amux-tui/src/client.rs"
    - "crates/amux-tui/src/main.rs"
    - "crates/amux-tui/src/app/keyboard.rs"
    - "crates/amux-tui/src/app/events.rs"
    - "frontend/src/lib/types.ts"
    - "frontend/src/lib/auditStore.ts"
    - "frontend/src/components/audit-panel/AuditRow.tsx"
    - "frontend/electron/preload.cjs"
    - "frontend/electron/main.cjs"

key-decisions:
  - "get_anticipatory_for_heartbeat() reads items + session_start_pending_at as a snapshot tuple for heartbeat merge"
  - "Morning brief flag cleared only AFTER successful LLM synthesis to avoid losing it on synthesis failure"
  - "Predictive hydration items annotated as LOW-PRIORITY INFORMATIONAL in synthesis prompt per D-09"
  - "Pattern change detection thresholds: >2 hour peak shift, <0.5 learned weight for deprioritization"
  - "TUI dismiss uses 'd' key with optimistic local update before IPC round-trip"
  - "Electron dismiss uses X button with hover opacity; dismissed entries at 50% opacity + strikethrough"

patterns-established:
  - "Anticipatory-heartbeat merge: Phase 2.5 in heartbeat cycle gathers anticipatory items and formats them for LLM synthesis"
  - "Learning transparency: Phase 2.6 detects pattern changes and appends natural-language observations to synthesis prompt"
  - "Optimistic dismiss: TUI and Electron mark entries dismissed locally before sending IPC to daemon"

requirements-completed: [BEAT-07, BEAT-09]

# Metrics
duration: 10min
completed: 2026-03-23
---

# Phase 4 Plan 3: Anticipatory Integration and Client Dismiss Summary

**M2 anticipatory items merged into heartbeat synthesis, learning transparency templates invoked on pattern changes, and audit dismiss actions wired in TUI and Electron**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-23T12:58:16Z
- **Completed:** 2026-03-23T13:08:16Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments
- Merged anticipatory runtime outputs (morning brief, stuck hints, predictive hydration) into the heartbeat LLM synthesis prompt as a dedicated Anticipatory Items section
- Morning brief augments only the first heartbeat of a new session and clears the pending flag after successful synthesis (D-08 / Pitfall 3)
- Added learning transparency explanation templates (schedule_learned, check_deprioritized, check_reprioritized) with natural-language output ("I've noticed you're usually active during...")
- Wired pattern-change detection: peak hours shift >2 hours triggers schedule_learned, learned weight below 0.5 triggers check_deprioritized
- Added 'd' key dismiss in TUI with dimmed/strikethrough visual feedback and daemon IPC
- Added dismiss button (X) on Electron AuditRow with optimistic update and contextBridge IPC

## Task Commits

Each task was committed atomically:

1. **Task 1: Merge anticipatory items and learning transparency into heartbeat synthesis** - `e202c8b` (feat)
2. **Task 2: Client dismiss actions in TUI and Electron + IPC bridge** - `793fe33` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/heartbeat.rs` - Phase 2.5 (anticipatory merge) and Phase 2.6 (learning transparency) added to run_structured_heartbeat_adaptive; synthesis prompt expanded with anticipatory, morning brief, and learning sections; morning brief flag clearing after successful synthesis
- `crates/amux-daemon/src/agent/anticipatory.rs` - get_anticipatory_for_heartbeat() method returning items + first-heartbeat flag
- `crates/amux-daemon/src/agent/explanation.rs` - schedule_learned, check_deprioritized, check_reprioritized templates + tests
- `crates/amux-tui/src/state/audit.rs` - dismissed field on AuditEntryVm, DismissEntry action, selected_entry_id accessor
- `crates/amux-tui/src/widgets/audit_feed.rs` - Dismissed entry styling (dimmed, strikethrough), dismiss hint in header
- `crates/amux-tui/src/state/mod.rs` - AuditDismiss DaemonCommand variant
- `crates/amux-tui/src/client.rs` - dismiss_audit_entry client method
- `crates/amux-tui/src/main.rs` - AuditDismiss DaemonCommand dispatch
- `crates/amux-tui/src/app/keyboard.rs` - 'd' key binding for audit dismiss
- `crates/amux-tui/src/app/events.rs` - dismissed: false on new AuditEntryVm construction
- `frontend/src/lib/types.ts` - AuditUserAction type and userAction field on AuditEntry
- `frontend/src/lib/auditStore.ts` - dismissEntry action with optimistic update + IPC bridge call
- `frontend/src/components/audit-panel/AuditRow.tsx` - Dismiss button, dismissed visual state (opacity + strikethrough)
- `frontend/electron/preload.cjs` - dismissAuditEntry exposed on contextBridge
- `frontend/electron/main.cjs` - dismiss-audit-entry IPC handler forwarding to daemon

## Decisions Made
- get_anticipatory_for_heartbeat() returns a (Vec<AnticipatoryItem>, bool) tuple -- simple snapshot that avoids holding the RwLock across the synthesis prompt construction
- Morning brief flag cleared only AFTER successful LLM synthesis (not before) to avoid losing the morning brief on synthesis failure per Pitfall 3
- Predictive hydration items annotated as "LOW-PRIORITY INFORMATIONAL" in the synthesis prompt per D-09 so the LLM knows not to treat them as actionable
- Pattern change detection for learning transparency uses simple thresholds: >2 hour symmetric difference for peak hours, <0.5 for learned weight deprioritization
- TUI 'd' key handler works from both Chat and Sidebar focus areas since audit entries may be visible in either context
- Electron dismiss uses optimistic update pattern (local state first, IPC fire-and-forget) matching the pattern used elsewhere in the codebase

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- InputNoticeKind::Info did not exist in TUI app -- used InputNoticeKind::Success instead for the dismiss confirmation notice (minor UI difference, no impact on functionality)

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 4 (Adaptive Heartbeat) is fully complete: all 3 plans executed
- The heartbeat is now activity-aware, priority-gated, learns from feedback, integrates anticipatory items, and communicates pattern changes
- All 262 TUI tests + daemon tests pass with zero regressions
- Full workspace compiles clean

## Self-Check: PASSED

- All 15 modified files exist on disk
- Commit e202c8b (Task 1) found in git log
- Commit 793fe33 (Task 2) found in git log
- SUMMARY.md created at expected path

---
*Phase: 04-adaptive-heartbeat*
*Completed: 2026-03-23*
