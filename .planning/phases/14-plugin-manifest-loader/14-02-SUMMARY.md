---
phase: 14-plugin-manifest-loader
plan: 02
subsystem: plugin
tags: [sqlite, json-schema, plugin-loader, ipc, validation]

requires:
  - phase: 14-01
    provides: PluginManifest struct, JSON Schema v1, Plugin* IPC variants

provides:
  - Plugin loader with two-phase validation (size + schema + structural limits)
  - SQLite persistence for plugin metadata (plugins, plugin_settings, plugin_credentials tables)
  - PluginManager struct for lifecycle management (load, list, get, enable/disable)
  - Server.rs IPC handlers for PluginList, PluginGet, PluginEnable, PluginDisable
  - Stale record reconciliation on startup

affects: [15-plugin-api-proxy, 16-plugin-settings-ui, 17-plugin-commands, 18-plugin-install-cli]

tech-stack:
  added: []
  patterns: [two-phase-manifest-validation, plugin-persistence-crud, plugin-manager-lifecycle]

key-files:
  created:
    - crates/amux-daemon/src/plugin/loader.rs
    - crates/amux-daemon/src/plugin/persistence.rs
  modified:
    - crates/amux-daemon/src/plugin/mod.rs
    - crates/amux-daemon/src/history.rs
    - crates/amux-daemon/src/server.rs

key-decisions:
  - "HistoryStore.conn made pub(crate) for cross-module SQLite access from plugin persistence"
  - "Plugin list_plugins reads from SQLite for accurate enabled state, not just in-memory cache"
  - "Settings schema extracted from stored manifest_json as standalone JSON string for future dynamic form rendering"

patterns-established:
  - "PluginPersistence wraps Arc<HistoryStore> for async SQLite access following existing daemon patterns"
  - "PluginManager initialized in server.rs run(), threaded through all accept loops to handle_connection"
  - "Stale record reconciliation: after disk scan, remove SQLite records with no matching disk manifest"

requirements-completed: [PLUG-02, PLUG-03, PLUG-08, PLUG-09]

duration: 8min
completed: 2026-03-24
---

# Phase 14 Plan 02: Plugin Loader and Server Wiring Summary

**Two-phase manifest validation, SQLite plugin persistence, PluginManager lifecycle, and four IPC handlers wired into daemon server**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-24T17:59:15Z
- **Completed:** 2026-03-24T18:07:51Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Plugin loader validates manifests with size gate (100KB), JSON Schema validation, and structural limits (50 endpoints, 30 settings)
- SQLite persistence layer with full CRUD (list, get, upsert, set_enabled, remove_stale_plugins) for plugin metadata
- PluginManager loads all valid manifests from disk on startup, persists to SQLite, reconciles stale records
- Four IPC handlers (PluginList, PluginGet, PluginEnable, PluginDisable) wired into server message dispatch
- 32 plugin-specific unit tests all passing; 762 total daemon tests green

## Task Commits

Each task was committed atomically:

1. **Task 1: Plugin loader, persistence, and PluginManager** - `781e333` (feat)
2. **Task 2: Server.rs PluginManager initialization and IPC handler wiring** - `8d1cd72` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/plugin/loader.rs` - Two-phase manifest validation, plugin name validation, directory scanning
- `crates/amux-daemon/src/plugin/persistence.rs` - SQLite CRUD for plugins table (list, get, upsert, set_enabled, remove_stale)
- `crates/amux-daemon/src/plugin/mod.rs` - PluginManager struct with load_all_from_disk, list, get, set_enabled methods
- `crates/amux-daemon/src/history.rs` - Three new tables (plugins, plugin_settings, plugin_credentials) in init_schema; conn field made pub(crate)
- `crates/amux-daemon/src/server.rs` - PluginManager creation in run(), threaded through accept loops, four IPC handlers wired

## Decisions Made
- HistoryStore.conn changed from private to pub(crate) so plugin persistence module can access SQLite connection directly, following the same `conn.call()` pattern used throughout history.rs
- PluginManager.list_plugins() reads from SQLite (not just in-memory HashMap) to ensure enabled state reflects IPC toggle operations
- Settings schema is extracted from stored manifest_json as a standalone JSON string, ready for Phase 16 dynamic form rendering
- Plugin name validation supports scoped names (@scope/name) returning the short name as the plugin ID

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Made HistoryStore.conn pub(crate)**
- **Found during:** Task 1 (persistence.rs implementation)
- **Issue:** The `conn` field on `HistoryStore` was private, preventing the plugin persistence module from accessing the SQLite connection
- **Fix:** Changed `conn: tokio_rusqlite::Connection` to `pub(crate) conn: tokio_rusqlite::Connection`
- **Files modified:** crates/amux-daemon/src/history.rs
- **Verification:** All 762 daemon tests pass, no visibility regressions
- **Committed in:** 781e333 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed instance_path method call syntax**
- **Found during:** Task 1 (loader.rs validation error formatting)
- **Issue:** `jsonschema` 0.45 uses `instance_path()` as a method, not a field
- **Fix:** Changed `e.instance_path` to `e.instance_path()`
- **Files modified:** crates/amux-daemon/src/plugin/loader.rs
- **Committed in:** 781e333 (Task 1 commit)

**3. [Rule 1 - Bug] Fixed tokio_rusqlite error type mismatch**
- **Found during:** Task 1 (persistence.rs set_enabled)
- **Issue:** Closure returns `tokio_rusqlite::Error` not `rusqlite::Error`; needed `.into()` conversion
- **Fix:** Changed `Err(rusqlite::Error::QueryReturnedNoRows)` to `Err(rusqlite::Error::QueryReturnedNoRows.into())`
- **Files modified:** crates/amux-daemon/src/plugin/persistence.rs
- **Committed in:** 781e333 (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All auto-fixes necessary for compilation. No scope creep.

## Issues Encountered
- Plan specified adding PRAGMA foreign_keys to init_schema, but it was already set in HistoryStore::new() at connection time. Skipped per plan instruction "Check if it already exists first -- if it does, skip."

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plugin infrastructure complete: manifests validated, persisted, and queryable via IPC
- Ready for Phase 15 (API proxy layer), Phase 16 (plugin settings UI), or Phase 18 (CLI install commands)
- All Plugin* IPC handlers functional and tested

## Self-Check: PASSED

- All 4 key files exist on disk
- Both task commits (781e333, 8d1cd72) verified in git log
- 32 plugin tests pass, 762 total daemon tests pass, 31 protocol tests pass

---
*Phase: 14-plugin-manifest-loader*
*Completed: 2026-03-24*
