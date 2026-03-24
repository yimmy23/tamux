---
phase: 15-plugin-cli-install
plan: 01
subsystem: api
tags: [ipc, bincode, plugin, sqlite, rust]

# Dependency graph
requires:
  - phase: 14-plugin-manifest-loader
    provides: "PluginManager, PluginPersistence, loader/manifest types, Plugin IPC list/get/enable/disable"
provides:
  - "PluginInstall and PluginUninstall ClientMessage IPC variants"
  - "PluginManager.register_plugin() -- validates manifest, checks conflicts, persists, updates in-memory"
  - "PluginManager.unregister_plugin() -- removes from SQLite (3 tables) and in-memory map"
  - "PluginManager.check_conflicts() -- rejects duplicate commands/skills across plugins (INST-07)"
  - "PluginPersistence.remove_plugin() -- deletes from plugins, plugin_settings, plugin_credentials"
  - "server.rs handlers wiring PluginInstall/PluginUninstall to PluginManager methods"
affects: [15-plugin-cli-install, 16-plugin-api-proxy, 17-plugin-ui-surfaces]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Plugin conflict detection pattern: iterate in-memory map comparing command keys and skill paths"
    - "Plugin register pattern: validate manifest, check conflicts, persist, update in-memory, return PluginInfo"

key-files:
  created: []
  modified:
    - "crates/amux-protocol/src/messages.rs"
    - "crates/amux-daemon/src/plugin/mod.rs"
    - "crates/amux-daemon/src/plugin/persistence.rs"
    - "crates/amux-daemon/src/server.rs"

key-decisions:
  - "Combined Task 1 and Task 2 into single commit due to Rust exhaustive match requirement"
  - "Conflict detection checks both command keys and skill paths across all loaded plugins"
  - "Re-install of same plugin (name match) skips conflict check to allow upgrades"

patterns-established:
  - "Plugin register/unregister pattern: validate -> conflict check -> persist -> in-memory update"

requirements-completed: [INST-04, INST-06, INST-07]

# Metrics
duration: 4min
completed: 2026-03-24
---

# Phase 15 Plan 01: Plugin Install/Uninstall IPC Summary

**PluginInstall/PluginUninstall IPC endpoints with conflict detection, SQLite persistence, and server.rs handler wiring**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-24T20:51:37Z
- **Completed:** 2026-03-24T20:56:32Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added PluginInstall and PluginUninstall IPC message variants with bincode roundtrip tests
- Implemented register_plugin (validate manifest, check conflicts, persist, update in-memory map)
- Implemented unregister_plugin (remove from all 3 SQLite tables and in-memory map)
- Implemented check_conflicts (detects duplicate command keys and skill paths across plugins per INST-07)
- Added remove_plugin persistence method deleting from plugins, plugin_settings, and plugin_credentials tables
- Wired both new message handlers in server.rs returning PluginActionResult responses

## Task Commits

Each task was committed atomically:

1. **Task 1+2: Add IPC messages, persistence, PluginManager methods, server handlers** - `9429fa2` (feat)

**Plan metadata:** [pending] (docs: complete plan)

_Note: Tasks 1 and 2 were combined into a single commit because Rust's exhaustive match requires all enum variants to be handled -- adding IPC variants without server.rs handlers causes compilation failure._

## Files Created/Modified
- `crates/amux-protocol/src/messages.rs` - Added PluginInstall/PluginUninstall ClientMessage variants + bincode roundtrip tests
- `crates/amux-daemon/src/plugin/persistence.rs` - Added remove_plugin method (deletes from 3 tables) + test
- `crates/amux-daemon/src/plugin/mod.rs` - Added register_plugin, unregister_plugin, check_conflicts, plugins_dir methods
- `crates/amux-daemon/src/server.rs` - Added PluginInstall/PluginUninstall match arms in handle_connection

## Decisions Made
- Combined Tasks 1 and 2 into a single commit because adding new ClientMessage variants requires exhaustive match handling in server.rs -- Rust won't compile without both changes
- Conflict detection uses in-memory plugin map (not SQLite) for accurate loaded state comparison
- Re-install detection skips conflict check for same-named plugins to support version upgrades
- unregister_plugin does NOT delete files from disk (CLI handles cleanup)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Combined Tasks 1 and 2 into single commit**
- **Found during:** Task 1 (adding IPC variants)
- **Issue:** Adding PluginInstall/PluginUninstall to ClientMessage enum without server.rs match arms causes E0004 non-exhaustive patterns error -- daemon won't compile
- **Fix:** Added all PluginManager methods (register_plugin, unregister_plugin, check_conflicts) and server.rs handlers alongside the IPC variants in the same commit
- **Files modified:** All 4 plan files
- **Verification:** cargo check --workspace passes, all tests pass
- **Committed in:** 9429fa2

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Task ordering adjusted for compilation requirements. All planned functionality delivered.

## Issues Encountered
None - all tests passed on first run after fixing the exhaustive match requirement.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plugin install/uninstall IPC endpoints ready for CLI consumption in Plan 15-02
- CLI can now send PluginInstall { dir_name, install_source } after copying plugin files to disk
- CLI can send PluginUninstall { name } and then clean up the plugin directory
- Conflict detection (INST-07) prevents duplicate command/skill registration

## Self-Check: PASSED

All files verified present. Commit 9429fa2 verified in git log. cargo check --workspace passes. 33 protocol tests + 763 daemon tests green.

---
*Phase: 15-plugin-cli-install*
*Completed: 2026-03-24*
