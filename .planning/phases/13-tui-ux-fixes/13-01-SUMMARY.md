---
phase: 13-tui-ux-fixes
plan: 01
subsystem: tui
tags: [ratatui, concierge, sidebar, heartbeat, chat]

# Dependency graph
requires: []
provides:
  - "Concierge onboarding renders as chat message in concierge thread (not overlay banner)"
  - "Sidebar shows last 3 recent autonomous actions from heartbeat digests"
affects: [13-tui-ux-fixes]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Concierge welcome injected as assistant message via ChatState thread mutation"
    - "RecentActionVm populated from HeartbeatDigest events, rendered in sidebar layout chunk"

key-files:
  created: []
  modified:
    - crates/amux-tui/src/app.rs
    - crates/amux-tui/src/app/events.rs
    - crates/amux-tui/src/app/rendering.rs
    - crates/amux-tui/src/widgets/sidebar.rs

key-decisions:
  - "concierge_banner_visible() returns false instead of deleting overlay code, preserving backward compat"
  - "Action buttons appended as text to injected chat message rather than custom inline widget"
  - "Recent actions populated from HeartbeatDigest events (existing pipeline) rather than adding new polling"

patterns-established:
  - "Overlay-to-chat migration: disable overlay via visible flag, inject content as chat message"
  - "Sidebar extension: add height chunk + lines function following gateway_status_lines pattern"

requirements-completed: [PRUX-04, PRUX-06]

# Metrics
duration: 3min
completed: 2026-03-24
---

# Phase 13 Plan 01: Concierge Chat Thread and Sidebar Recent Actions Summary

**Concierge onboarding moved from clipped overlay to concierge conversation thread, sidebar shows last 3 heartbeat-driven autonomous actions**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-24T11:49:03Z
- **Completed:** 2026-03-24T11:52:58Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Concierge onboarding message now renders as a regular assistant message in the concierge thread, visible in chat scroll history
- Overlay banner effectively disabled (concierge_banner_visible returns false) -- layout no longer allocates space for it
- TUI sidebar displays up to 3 recent autonomous actions below gateway status, with per-type icons and truncated summaries
- Recent actions populated from HeartbeatDigest events via existing event pipeline (no new daemon polling)

## Task Commits

Each task was committed atomically:

1. **Task 1: Route concierge onboarding into chat thread instead of overlay banner** - `48a054d` (feat)
2. **Task 2: Add recent autonomous actions to TUI sidebar** - `306ab1e` (feat)

## Files Created/Modified
- `crates/amux-tui/src/app.rs` - Added RecentActionVm struct, recent_actions field on TuiModel, concierge_banner_visible returns false
- `crates/amux-tui/src/app/events.rs` - ConciergeWelcome handler injects content as chat message; HeartbeatDigest populates recent_actions
- `crates/amux-tui/src/app/rendering.rs` - Sidebar render call passes recent_actions
- `crates/amux-tui/src/widgets/sidebar.rs` - Added recent_actions_lines function and layout chunk, render signature extended

## Decisions Made
- concierge_banner_visible() returns false instead of removing overlay code -- simpler change, preserves mouse.rs and rendering.rs backward compat
- Action buttons appended as text to the injected chat message (e.g., "Suggested actions: [Explore goals] [Start a task]") rather than building a custom inline widget -- existing keyboard shortcuts still work via ConciergeState
- Recent actions populated from HeartbeatDigest events rather than adding new status polling -- reuses existing data pipeline with zero daemon changes

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- TUI concierge experience ready for UAT validation
- Sidebar recent actions will show data as soon as heartbeat digests arrive from daemon
- Overlay panel code preserved but inactive -- can be removed in a follow-up cleanup if desired

## Self-Check: PASSED

All created/modified files verified. All commit hashes found in git log.

---
*Phase: 13-tui-ux-fixes*
*Completed: 2026-03-24*
