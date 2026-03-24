---
phase: 12-cli-polish
plan: 01
subsystem: cli
tags: [clap, rust, binary-detection, cli-ux]

# Dependency graph
requires:
  - phase: 11-setup-wizard-rewrite
    provides: "CLI entry point with setup wizard integration"
provides:
  - "tamux tui launch command with sibling binary detection"
  - "tamux gui launch command with platform fallback"
  - "tamux stats alias for status display"
  - "Hidden internal commands (new, attach, scrub) from --help"
  - "Audit entry ID display for --detail workflows"
  - "find_sibling_binary helper for binary discovery"
affects: [12-cli-polish, 13-tui-ux-fixes]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Sibling binary detection: check current_exe parent dir, then PATH fallback"
    - "TAMUX_GUI_PATH env var override for GUI binary location"
    - "Platform-conditional compilation (#[cfg(target_os)]) for GUI fallback"
    - "Combined match arms for command aliases (Status | Stats)"

key-files:
  created: []
  modified:
    - "crates/amux-cli/src/main.rs"

key-decisions:
  - "find_sibling_binary checks exe parent dir first, then bare name for PATH lookup -- no which dependency added"
  - "GUI launch tries TAMUX_GUI_PATH env var, then tamux-desktop sibling, then platform fallback (macOS open -a)"
  - "Stats implemented as combined match arm with Status rather than delegation function"

patterns-established:
  - "Sibling binary detection pattern for cross-binary launches within tamux suite"
  - "Platform-specific fallback with #[cfg] for desktop app discovery"

requirements-completed: [DIST-01]

# Metrics
duration: 1min
completed: 2026-03-24
---

# Phase 12 Plan 01: CLI Polish Summary

**Add tui/gui launch commands, stats alias, hide internal commands, and fix audit ID display in CLI main.rs**

## Performance

- **Duration:** 1 min
- **Started:** 2026-03-24T11:47:50Z
- **Completed:** 2026-03-24T11:48:54Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- Added `tamux tui` command that spawns `tamux-tui` via sibling binary detection then PATH fallback
- Added `tamux gui` command that spawns desktop GUI with TAMUX_GUI_PATH env, sibling binary, and platform fallback
- Added `tamux stats` as alias for `tamux status` via combined match arm
- Hidden internal commands (New, Attach, Scrub) from `--help` with `#[command(hide = true)]`
- Fixed audit ID display by prepending `entry.id` as first field in `print_audit_row` output

## Task Commits

Each task was committed atomically:

1. **Task 1: Add launch commands, hide internals, fix audit IDs, add stats alias** - `cba3a84` (feat)

## Files Created/Modified
- `crates/amux-cli/src/main.rs` - Added Tui/Gui/Stats command variants, find_sibling_binary helper, hidden internal commands, fixed audit row format

## Decisions Made
- Used `find_sibling_binary` helper that checks `current_exe()` parent directory first, then falls back to bare name for OS PATH lookup -- avoids adding `which` as a dependency
- GUI launch uses TAMUX_GUI_PATH env var as first priority, then `tamux-desktop` sibling binary, then platform-specific fallback (macOS `open -a tamux`)
- Stats command implemented as combined match arm `Commands::Status | Commands::Stats =>` rather than extracting a helper function -- simpler and idiomatic

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CLI launch commands ready for end users
- Settings subcommand (Plan 12-02) can proceed independently
- All five UAT fixes from this plan are complete and verified

## Self-Check: PASSED

- FOUND: crates/amux-cli/src/main.rs
- FOUND: commit cba3a84
- FOUND: 12-01-SUMMARY.md

---
*Phase: 12-cli-polish*
*Completed: 2026-03-24*
