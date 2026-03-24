---
phase: 15-plugin-cli-install
plan: 02
subsystem: cli
tags: [rust, clap, ipc, npm, github, plugin-install, plugin-uninstall]

# Dependency graph
requires:
  - phase: 15-plugin-cli-install/01
    provides: PluginInstall/PluginUninstall IPC message variants, daemon handler wiring
  - phase: 14-plugin-manifest-loader
    provides: Plugin manifest format, PluginManager, PluginInfo, PluginList/Enable/Disable IPC
provides:
  - "tamux plugin add <source> CLI command with npm/GitHub/local auto-detection"
  - "tamux plugin remove <name> CLI command with daemon deregistration + file cleanup"
  - "tamux plugin ls CLI command showing installed plugins table"
  - "tamux plugin enable/disable CLI commands for toggling plugins via IPC"
  - "detect_source_type() function for source auto-detection"
  - "install_plugin_v2() unified install entry point"
  - "remove_plugin_files() disk cleanup function"
  - "send_plugin_install/uninstall/list/enable/disable IPC helper functions"
affects: [16-plugin-api-proxy, 17-plugin-oauth, 18-plugin-commands, 19-plugin-skills, 20-gmail-calendar-validation]

# Tech tracking
tech-stack:
  added: [reqwest-blocking]
  patterns: [source-auto-detection, two-step-install-ipc, graceful-daemon-unreachable]

key-files:
  created: []
  modified:
    - crates/amux-cli/src/plugins.rs
    - crates/amux-cli/src/client.rs
    - crates/amux-cli/src/main.rs
    - crates/amux-cli/Cargo.toml

key-decisions:
  - "reqwest::blocking::Client for GitHub tarball download to avoid block_on panic inside tokio runtime"
  - "Two-step install: files first, then IPC registration; failed registration triggers file cleanup"
  - "Daemon-unreachable during install prints warning but keeps files; during uninstall removes files anyway"

patterns-established:
  - "Source auto-detection: github.com/git@/.git -> GitHub, existing dir -> Local, fallback -> npm"
  - "Plugin IPC round-trip pattern: send_plugin_* functions matching send_skill_* convention"

requirements-completed: [INST-01, INST-02, INST-03, INST-04, INST-05, INST-06, INST-08]

# Metrics
duration: 4min
completed: 2026-03-24
---

# Phase 15 Plan 02: Plugin CLI Install Summary

**CLI plugin management with npm/GitHub/local source detection, two-step install-then-register flow, and ls/enable/disable subcommands**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-24T20:59:40Z
- **Completed:** 2026-03-24T21:04:06Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Complete `tamux plugin add/remove/ls/enable/disable` CLI commands wired to daemon IPC
- Source auto-detection (npm/GitHub/local) with GitHub URL parsing supporting HTTPS, SSH, and shorthand
- GitHub install with git clone first, reqwest::blocking tarball fallback for environments without git
- Graceful handling when daemon is unreachable for both install and uninstall operations
- 5 unit tests for source detection and GitHub URL parsing

## Task Commits

Each task was committed atomically:

1. **Task 1: Rewrite plugins.rs with source detection, install flows, and uninstall** - `f73dc44` (feat)
2. **Task 2: Wire Plugin subcommand in main.rs with IPC client helpers** - `04e6c08` (feat)

## Files Created/Modified
- `crates/amux-cli/src/plugins.rs` - Added PluginSource enum, detect_source_type, install_from_npm/github/local, install_plugin_v2, remove_plugin_files, parse_github_url, copy_dir_recursive
- `crates/amux-cli/src/client.rs` - Added send_plugin_install/uninstall/list/enable/disable IPC helpers
- `crates/amux-cli/src/main.rs` - Added PluginAction enum and Commands::Plugin match arm with add/remove/ls/enable/disable
- `crates/amux-cli/Cargo.toml` - Moved tempfile to runtime deps, added reqwest blocking feature

## Decisions Made
- Used reqwest::blocking::Client for GitHub tarball download because the CLI runs inside #[tokio::main] and calling block_on() inside an existing tokio runtime panics
- Two-step install (files then IPC) allows plugins to be installed even when daemon is down, with warning
- Failed daemon registration triggers automatic file cleanup to prevent orphaned plugin directories
- Uninstall sends IPC first (daemon cleanup), then removes files -- if daemon unreachable, files still removed with warning

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plugin CLI install/uninstall fully operational, ready for Phase 16+ (API proxy, OAuth, commands, skills)
- All existing tests remain green (11/11 in tamux-cli, full workspace compiles)
- Plugin subcommands appear in `tamux plugin --help`

## Self-Check: PASSED

---
*Phase: 15-plugin-cli-install*
*Completed: 2026-03-24*
