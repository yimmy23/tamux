---
phase: 14-plugin-manifest-loader
plan: 01
subsystem: plugin
tags: [serde, jsonschema, bincode, manifest, ipc, plugin-system]

# Dependency graph
requires: []
provides:
  - PluginManifest serde structs for plugin.json deserialization (manifest.rs)
  - Embedded JSON Schema v1 validator for manifest validation (schema.rs)
  - Plugin IPC message variants (PluginList, PluginGet, PluginEnable, PluginDisable) in protocol
  - PluginInfo wire type for cross-crate plugin metadata transfer
  - Stub IPC handlers in server.rs for Plan 14-02 to wire
affects: [14-02-PLAN, plugin-loader, plugin-persistence, plugin-settings]

# Tech tracking
tech-stack:
  added: [jsonschema 0.45, semver 1]
  patterns: [permissive-deserialization-with-flatten, embedded-json-schema-validator, plugin-ipc-pattern]

key-files:
  created:
    - crates/amux-daemon/src/plugin/mod.rs
    - crates/amux-daemon/src/plugin/manifest.rs
    - crates/amux-daemon/src/plugin/schema.rs
  modified:
    - crates/amux-daemon/Cargo.toml
    - crates/amux-daemon/src/main.rs
    - crates/amux-protocol/src/messages.rs
    - crates/amux-daemon/src/server.rs

key-decisions:
  - "jsonschema 0.45 for JSON Schema Draft 2020-12 validation; compile once, reuse validator"
  - "Permissive deserialization with serde(flatten) captures unknown fields per D-01"
  - "Stub IPC handlers return empty/false responses until Plan 14-02 wires PluginManager"

patterns-established:
  - "Plugin module at crate::plugin (daemon-level, not inside agent/) to avoid circular dependency"
  - "PluginInfo wire type mirrors manifest metadata fields for lean IPC payloads"
  - "Plugin* IPC variants appended at end of ClientMessage/DaemonMessage enums for bincode compat"

requirements-completed: [PLUG-01, PLUG-04, PLUG-05, PLUG-06, PLUG-07]

# Metrics
duration: 7min
completed: 2026-03-24
---

# Phase 14 Plan 01: Plugin Manifest Types Summary

**PluginManifest serde structs with permissive deserialization, embedded JSON Schema v1 validator, and Plugin IPC message variants with bincode roundtrip tests**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-24T17:49:15Z
- **Completed:** 2026-03-24T17:56:36Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Full plugin manifest type system: PluginManifest, SettingField, ApiSection, EndpointDef, RateLimitDef, CommandDef, AuthSection structs
- Embedded JSON Schema v1 with compile_schema_v1() factory for reusable validation
- Plugin IPC message variants in both ClientMessage and DaemonMessage enums with PluginInfo wire type
- 20 total tests (11 manifest/schema + 9 IPC bincode/JSON roundtrips) all passing
- Full workspace compilation verified with no errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Plugin manifest serde structs and JSON Schema v1** - `c9e23bb` (feat)
2. **Task 2: Plugin IPC message variants and PluginInfo wire type** - `d0661cd` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/plugin/mod.rs` - Plugin module declaration (manifest, schema)
- `crates/amux-daemon/src/plugin/manifest.rs` - Serde structs for plugin.json with 6 unit tests
- `crates/amux-daemon/src/plugin/schema.rs` - Embedded JSON Schema v1 and validator factory with 5 unit tests
- `crates/amux-daemon/Cargo.toml` - Added jsonschema 0.45 and semver 1 dependencies
- `crates/amux-daemon/src/main.rs` - Added `pub mod plugin` declaration
- `crates/amux-protocol/src/messages.rs` - Plugin* IPC variants, PluginInfo struct, 9 roundtrip tests
- `crates/amux-daemon/src/server.rs` - Stub match arms for Plugin* client messages

## Decisions Made
- Used jsonschema 0.45 (not 0.28 from initial Cargo.toml edit) matching latest crates.io availability
- Plugin module placed at `crate::plugin` (daemon-level, not inside agent/) per anti-pattern guidance to avoid circular dependency when AgentEngine needs PluginManager
- Stub IPC handlers return empty/false responses (PluginList returns empty vec, PluginGet returns None, Enable/Disable return failure) until Plan 14-02 wires PluginManager
- Schema uses `"const": 1` for schema_version to enforce v1 only; name pattern `^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$` prevents path traversal

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added stub IPC handlers in server.rs**
- **Found during:** Task 2 (Plugin IPC message variants)
- **Issue:** New ClientMessage variants caused non-exhaustive match error in server.rs, preventing compilation
- **Fix:** Added stub match arms for PluginList, PluginGet, PluginEnable, PluginDisable that return empty/error responses
- **Files modified:** crates/amux-daemon/src/server.rs
- **Verification:** `cargo check` passes for full workspace
- **Committed in:** d0661cd (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for compilation. Stub handlers will be replaced with real PluginManager dispatch in Plan 14-02.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All type contracts established for Plan 14-02 (loader, persistence, server wiring)
- PluginManifest structs ready for manifest deserialization
- compile_schema_v1() ready for validation in loader
- Plugin IPC variants ready for handler wiring
- Stub handlers in server.rs provide safe defaults until real handlers are connected

## Self-Check: PASSED

All files exist, both commits verified, all acceptance criteria content confirmed.

---
*Phase: 14-plugin-manifest-loader*
*Completed: 2026-03-24*
