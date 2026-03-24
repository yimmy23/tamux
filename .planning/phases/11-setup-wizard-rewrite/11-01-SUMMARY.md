---
phase: 11-setup-wizard-rewrite
plan: 01
subsystem: cli
tags: [crossterm, ipc, setup-wizard, daemon-config, arrow-key-navigation]

# Dependency graph
requires:
  - phase: 10-progressive-ux
    provides: "AgentSetTierOverride IPC, capability tier system"
provides:
  - "IPC-based setup wizard with crossterm navigation"
  - "needs_setup_via_ipc() for daemon-based setup detection"
  - "needs_setup_legacy() fallback for first-ever-run"
affects: [11-02, cli, tui, daemon]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Wizard connects to daemon via IPC socket for all config operations"
    - "Arrow-key navigation via crossterm select_list helper"
    - "Masked text input via crossterm text_input helper"
    - "Two-phase setup detection: IPC first, legacy config.json read as fallback"

key-files:
  created: []
  modified:
    - crates/amux-cli/src/setup_wizard.rs
    - crates/amux-cli/src/main.rs

key-decisions:
  - "Auth source string read from ProviderAuthState rather than hardcoded mapping"
  - "Legacy needs_setup_at kept as read-only fallback for first-ever-run when daemon not started"
  - "AgentSetTierOverride treated as fire-and-forget with 100ms delay for daemon processing"

patterns-established:
  - "Wizard IPC pattern: wizard_connect/wizard_send/wizard_recv for long-lived framed connections"
  - "Crossterm select_list pattern: reusable arrow-key selection with wrapping"

requirements-completed: [DIST-05, PRUX-01]

# Metrics
duration: 4min
completed: 2026-03-24
---

# Phase 11 Plan 01: Setup Wizard Rewrite Summary

**Complete rewrite of setup_wizard.rs replacing config.json writes with IPC-based daemon config via AgentLoginProvider, AgentSetConfigItem, AgentSetTierOverride, and AgentValidateProvider messages, with crossterm arrow-key navigation**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-24T10:24:43Z
- **Completed:** 2026-03-24T10:28:55Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Setup wizard connects to daemon via IPC socket for all config operations -- never writes config.json
- Provider list queried from daemon at runtime via AgentGetProviderAuthStates (no hardcoded PROVIDERS array)
- All selections use crossterm arrow-key navigation (select_list helper with wrapping)
- Tier self-assessment, API key, provider selection, and connectivity test all go through daemon IPC
- main.rs uses two-phase setup detection (IPC first, legacy fallback) and simplified wizard flow

## Task Commits

Each task was committed atomically:

1. **Task 1: Rewrite setup_wizard.rs with IPC-based config and crossterm navigation** - `36d6fd6` (feat)
2. **Task 2: Update main.rs to use IPC-based setup detection and wizard flow** - `e3a36f0` (feat)

## Files Created/Modified
- `crates/amux-cli/src/setup_wizard.rs` - Complete rewrite: IPC connection helpers, crossterm select_list/text_input, provider query from daemon, tier/provider/key/validation all via IPC
- `crates/amux-cli/src/main.rs` - IPC-based setup detection with legacy fallback, removed SetupResult/preferred_client references, simplified Setup command

## Decisions Made
- Auth source string comes from ProviderAuthState returned by daemon rather than hardcoding an ApiType mapping
- Legacy needs_setup_at() kept as read-only config.json check for first-ever-run bootstrap case when daemon isn't started yet
- AgentSetTierOverride is fire-and-forget (no response), brief 100ms delay added for daemon processing
- Wizard handles daemon startup internally (ensure_daemon_running polls for 5 seconds), removing duplicate daemon spawn logic from main.rs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated main.rs call sites in Task 1**
- **Found during:** Task 1 (setup_wizard.rs rewrite)
- **Issue:** Changing run_setup_wizard() return type from Result<SetupResult> to Result<()> broke main.rs compilation
- **Fix:** Updated main.rs call sites to work with new API (removed SetupResult field access, simplified Setup command)
- **Files modified:** crates/amux-cli/src/main.rs
- **Verification:** cargo build -p tamux-cli succeeds
- **Committed in:** 36d6fd6 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for build to succeed. Task 2 then refined the main.rs changes further.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plan 11-02 (optional wizard steps, security defaults) can build on this foundation
- The select_list and text_input helpers are reusable for additional wizard steps
- Tier string is stored locally during wizard for controlling which optional steps to show

## Self-Check: PASSED

All files exist, all commits verified.

---
*Phase: 11-setup-wizard-rewrite*
*Completed: 2026-03-24*
