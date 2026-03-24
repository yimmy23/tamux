---
phase: 02-core-heartbeat
plan: 04
subsystem: agent, tui, frontend
tags: [heartbeat, rust, typescript, tui, electron, notification, agent-event, gap-closure]

# Dependency graph
requires:
  - phase: 02-core-heartbeat plan 03
    provides: "HeartbeatDigest event broadcast from daemon via AgentEvent channel"
  - phase: 02-core-heartbeat plan 01
    provides: "HeartbeatDigestItem, HeartbeatCheckType types; AgentEvent::HeartbeatDigest variant"
provides:
  - "TUI receives and renders HeartbeatDigest events in sidebar task tree (Zone 4)"
  - "Electron frontend pushes actionable HeartbeatDigest events to notification store"
  - "HeartbeatDigestVm/HeartbeatDigestItemVm view-model types in TUI state"
  - "NotificationSource 'heartbeat' variant in frontend types"
affects: [phase 04 adaptive heartbeat, phase 08 gateway response tracking]

# Tech tracking
tech-stack:
  added: []
  patterns: [ClientEvent dispatch to TaskState reduction for structured agent events, priority-sorted notification body construction]

key-files:
  created: []
  modified:
    - "crates/amux-tui/src/client.rs"
    - "crates/amux-tui/src/state/task.rs"
    - "crates/amux-tui/src/app/events.rs"
    - "crates/amux-tui/src/projection.rs"
    - "crates/amux-tui/src/widgets/task_tree.rs"
    - "frontend/src/App.tsx"
    - "frontend/src/lib/types.ts"

key-decisions:
  - "Used accent_secondary (amber) for priority-2 indicators instead of non-existent accent_warn theme field"
  - "HeartbeatDigest items represented as tuple (u8, String, String, String) in ClientEvent for lightweight cross-layer transport"
  - "Non-actionable digests silently ignored in both clients preserving BEAT-03 silent-by-default behavior"

patterns-established:
  - "HeartbeatDigestVm pattern: view-model struct in TUI state with accessor on TaskState for widget rendering"
  - "Agent event to notification store pattern: useNotificationStore.getState().addNotification() from onAgentEvent callback"

requirements-completed: [BEAT-04]

# Metrics
duration: 4min
completed: 2026-03-23
---

# Phase 02 Plan 04: HeartbeatDigest Client Rendering (Gap Closure) Summary

**Wire HeartbeatDigest event rendering into TUI sidebar (Zone 4) and Electron notification panel, closing BEAT-04 last-mile delivery gap**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-23T08:32:24Z
- **Completed:** 2026-03-23T08:36:32Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Wired HeartbeatDigest event through full TUI pipeline: client.rs dispatch -> TaskState reduction -> sidebar task tree rendering with priority indicators
- Added heartbeat_digest handler in Electron App.tsx that pushes actionable digest items to notification store with priority-sorted body
- Preserved silent-by-default behavior: non-actionable digests produce no visible notification in either client
- Closed BEAT-04 verification gap: HeartbeatDigest events are now rendered to users in both TUI and Electron surfaces
- 240 TUI tests pass with 0 regressions (including new heartbeat_digest_received_stores_and_replaces test)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire HeartbeatDigest through TUI client, state, events, and sidebar rendering** - `9f141a7` (feat)
2. **Task 2: Wire HeartbeatDigest into Electron frontend notification system** - `7cc9f3b` (feat)

## Files Created/Modified
- `crates/amux-tui/src/state/task.rs` - HeartbeatDigestVm/HeartbeatDigestItemVm structs, TaskAction::HeartbeatDigestReceived variant, last_digest field/accessor on TaskState, reduce handler, unit test
- `crates/amux-tui/src/client.rs` - ClientEvent::HeartbeatDigest variant, "heartbeat_digest" arm in dispatch_agent_event parsing items/cycle_id/actionable/digest/checked_at
- `crates/amux-tui/src/projection.rs` - ClientEvent::HeartbeatDigest variant, empty projection mapping (handled in events.rs)
- `crates/amux-tui/src/app/events.rs` - ClientEvent::HeartbeatDigest handler converting tuples to HeartbeatDigestItemVm, reducing into TaskState, updating status_line for actionable items
- `crates/amux-tui/src/widgets/task_tree.rs` - Zone 4 heartbeat digest rendering with priority indicators (!! danger, ! amber, dim default) and suggestion lines
- `frontend/src/App.tsx` - heartbeat_digest event handler in onAgentEvent listener pushing to notificationStore with priority-sorted body
- `frontend/src/lib/types.ts` - Added "heartbeat" to NotificationSource union type

## Decisions Made
- Used `accent_secondary` (amber) for priority-2 warning indicators since the TUI theme has no `accent_warn` field -- the plan referenced a non-existent theme style
- Represented HeartbeatDigest items as `Vec<(u8, String, String, String)>` tuples in ClientEvent for lightweight transport between client.rs and events.rs, converting to HeartbeatDigestItemVm structs in the event handler
- Non-actionable digests (actionable=false or empty items) are silently ignored in both clients, preserving BEAT-03 silent-by-default behavior

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Used accent_secondary instead of non-existent accent_warn**
- **Found during:** Task 1 (sidebar rendering)
- **Issue:** Plan specified `theme.accent_warn` for priority-2 indicators, but the TUI theme only has `accent_primary`, `accent_assistant`, `accent_secondary`, `accent_success`, and `accent_danger` -- no `accent_warn` exists
- **Fix:** Used `theme.accent_secondary` (amber) which is the closest semantic match for warning-level priority
- **Files modified:** `crates/amux-tui/src/widgets/task_tree.rs`
- **Verification:** `cargo check -p tamux-tui` compiles successfully
- **Committed in:** 9f141a7

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Trivial theme field substitution. No scope or behavior change.

## Issues Encountered
None beyond the auto-fixed theme field deviation above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all planned functionality is fully implemented.

## Next Phase Readiness
- BEAT-04 fully satisfied: HeartbeatDigest events rendered to users in both TUI and Electron
- All Phase 2 success criteria now met (5/5 truths verified)
- Ready for Phase 4 (adaptive heartbeat): operator model integration can build on the rendering pipeline established here
- check_unreplied_messages still returns items_found=0 (intentional Phase 2 stub from Plan 01) -- full detection deferred to Phase 8

## Self-Check: PASSED

All files exist, all commits verified.

---
*Phase: 02-core-heartbeat*
*Completed: 2026-03-23*
