---
phase: 16-plugin-settings-ui
plan: 03
subsystem: tui
tags: [ratatui, tui-settings, plugin-ui, ipc, daemon-command, settings-tab]

# Dependency graph
requires:
  - phase: 16-plugin-settings-ui
    provides: "Plugin settings IPC pipeline (PluginGetSettings, PluginUpdateSettings, PluginTestConnection), TUI client methods"
  - phase: 14-plugin-manifest
    provides: "PluginManager, PluginInfo struct, plugin persistence"
provides:
  - "SettingsTab::Plugins variant at index 11 with TAB_LABELS 'Plug'"
  - "PluginSettingsState, PluginListItem, PluginSchemaField structs for dynamic plugin state"
  - "render_plugins_tab widget with list mode and detail mode rendering"
  - "DaemonCommand plugin variants and main.rs dispatch wiring"
  - "ClientEvent plugin variants with DaemonMessage-to-ClientEvent dispatch in client.rs"
  - "Event handlers in events.rs populating PluginSettingsState from daemon responses"
  - "handle_plugins_settings_key in settings_handlers.rs for full list/detail mode keyboard navigation"
  - "Plugin tab fetch trigger on tab switch (keyboard and mouse)"
affects: [17-plugin-api-proxy, 18-plugin-oauth2]

# Tech tracking
tech-stack:
  added: []
  patterns: ["Separate PluginSettingsState from SettingsState for dynamic plugin data", "DaemonCommand enum for plugin IPC routing through main.rs dispatcher"]

key-files:
  created: []
  modified:
    - "crates/amux-tui/src/state/settings.rs"
    - "crates/amux-tui/src/widgets/settings.rs"
    - "crates/amux-tui/src/app.rs"
    - "crates/amux-tui/src/app/events.rs"
    - "crates/amux-tui/src/app/modal_handlers.rs"
    - "crates/amux-tui/src/app/mouse.rs"
    - "crates/amux-tui/src/app/rendering.rs"
    - "crates/amux-tui/src/app/settings_handlers.rs"
    - "crates/amux-tui/src/client.rs"
    - "crates/amux-tui/src/main.rs"
    - "crates/amux-tui/src/state/mod.rs"

key-decisions:
  - "PluginSettingsState stored separately on TuiModel (not inside SettingsState) because plugin data is dynamic and varies by installed plugins"
  - "DaemonCommand enum extended with 7 plugin variants rather than calling client methods directly, following the established TUI architecture pattern"
  - "Plugin list fetched on tab switch (not on connect) to avoid unnecessary IPC traffic when user never visits the Plugins tab"

patterns-established:
  - "Separate state struct for dynamic tab content: PluginSettingsState lives on TuiModel, not SettingsState"
  - "Custom key handler per tab: handle_plugins_settings_key follows handle_auth_settings_key and handle_subagent_settings_key pattern"

requirements-completed: [PSET-02, PSET-03]

# Metrics
duration: 10min
completed: 2026-03-24
---

# Phase 16 Plan 03: TUI Plugins Settings Tab Summary

**TUI Plugins settings tab with list/detail modes, dynamic schema field editing, enable/disable toggles, test connection, and full IPC wiring through DaemonCommand dispatch**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-24T22:10:58Z
- **Completed:** 2026-03-24T22:21:18Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments
- TUI settings panel has a "Plug" tab (index 11) with list mode showing installed plugins with enable/disable indicators and abbreviated auth status (N/A/OK/Exp per TUI copywriting contract)
- Detail mode renders dynamic settings fields from manifest schema with inline editing, secret masking, required field markers, and Test Connection/Connect action buttons
- Full IPC pipeline: 5 ClientEvent variants, 7 DaemonCommand variants, DaemonMessage dispatch in client.rs, event handlers in events.rs, and DaemonCommand routing in main.rs
- Keyboard (handle_plugins_settings_key) and mouse handlers for complete list/detail mode navigation

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend SettingsState with Plugins tab and PluginSettingsState** - `59ef45f` (feat)
2. **Task 2: Plugins tab widget rendering, TuiModel wiring, and settings_handlers** - `3e57614` (feat)

## Files Created/Modified
- `crates/amux-tui/src/state/settings.rs` - SettingsTab::Plugins variant, PluginSettingsState/PluginListItem/PluginSchemaField structs, updated tests
- `crates/amux-tui/src/widgets/settings.rs` - TAB_LABELS[12] with "Plug", render_plugins_tab function, exhaustive match arms for Plugins
- `crates/amux-tui/src/app.rs` - plugin_settings field on TuiModel, settings_tab_label for Plugins
- `crates/amux-tui/src/app/events.rs` - ClientEvent handlers for PluginList/PluginGet/PluginSettings/PluginTestConnection/PluginAction
- `crates/amux-tui/src/app/modal_handlers.rs` - Plugins tab key dispatch and PluginList fetch on tab switch
- `crates/amux-tui/src/app/mouse.rs` - PluginList fetch on mouse tab click
- `crates/amux-tui/src/app/rendering.rs` - plugin_settings threaded through to settings::render()
- `crates/amux-tui/src/app/settings_handlers.rs` - handle_plugins_settings_key with list/detail mode navigation, field editing, toggle, test connection
- `crates/amux-tui/src/client.rs` - 5 ClientEvent variants for plugin responses, DaemonMessage dispatch arms
- `crates/amux-tui/src/main.rs` - DaemonCommand dispatch for 7 plugin command variants
- `crates/amux-tui/src/state/mod.rs` - 7 DaemonCommand plugin variants

## Decisions Made
- PluginSettingsState stored separately on TuiModel rather than inside SettingsState because plugin data is dynamic and schema-driven, unlike the static field structure of other tabs
- Extended DaemonCommand enum with 7 plugin variants (PluginList, PluginGet, PluginEnable, PluginDisable, PluginGetSettings, PluginUpdateSetting, PluginTestConnection) following the established TUI architecture where TuiModel communicates with daemon through a DaemonCommand channel dispatched in main.rs
- Plugin list fetched on tab switch rather than on daemon connect to avoid unnecessary IPC when user never visits the Plugins tab
- Borrow checker fix: cloned plugin name before mutating test_result to satisfy Rust's exclusive borrow rules in the Test Connection handler

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan referenced app/mod.rs but actual file is app.rs**
- **Found during:** Task 2 (reading file structure)
- **Issue:** Plan references `crates/amux-tui/src/app/mod.rs` for TuiModel, but actual location is `crates/amux-tui/src/app.rs`
- **Fix:** Used correct file path for all modifications
- **Files modified:** crates/amux-tui/src/app.rs
- **Verification:** Build succeeds
- **Committed in:** 3e57614

**2. [Rule 3 - Blocking] DaemonCommand variants needed for plugin IPC routing**
- **Found during:** Task 2 (wiring event handlers)
- **Issue:** Plan assumed direct client method calls from event handlers, but TUI architecture routes through DaemonCommand enum dispatched in main.rs
- **Fix:** Added 7 DaemonCommand variants and corresponding dispatch in main.rs
- **Files modified:** crates/amux-tui/src/state/mod.rs, crates/amux-tui/src/main.rs
- **Verification:** Build and tests pass
- **Committed in:** 3e57614

**3. [Rule 1 - Bug] Borrow checker conflict in Test Connection handler**
- **Found during:** Task 2 (compilation)
- **Issue:** Cannot assign to `self.plugin_settings.test_result` while `self.plugin_settings` is borrowed through `selected_plugin()`
- **Fix:** Clone plugin name into local variable before mutating test_result
- **Files modified:** crates/amux-tui/src/app/settings_handlers.rs
- **Verification:** cargo build succeeds
- **Committed in:** 3e57614

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 bug)
**Impact on plan:** All auto-fixes necessary for correctness within the TUI architecture. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- TUI Plugins settings tab complete; plugin configuration is now accessible from both TUI and Electron surfaces
- Phase 16 (plugin-settings-ui) fully complete with all 3 plans delivered
- Ready for Phase 17 (plugin-api-proxy) and Phase 18 (plugin-oauth2) which will add API proxying and OAuth2 authentication flows that the Plugins tab can surface

## Self-Check: PASSED

All 11 modified files verified present. Both task commits (59ef45f, 3e57614) verified in git history.

---
*Phase: 16-plugin-settings-ui*
*Completed: 2026-03-24*
