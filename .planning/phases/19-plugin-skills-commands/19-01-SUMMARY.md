---
phase: 19-plugin-skills-commands
plan: 01
subsystem: plugin
tags: [plugin, skills, commands, ipc, yaml, slash-commands]

# Dependency graph
requires:
  - phase: 14-plugin-manifest
    provides: "PluginManager, LoadedPlugin, manifest types, plugin persistence"
  - phase: 17-plugin-api-proxy
    provides: "API proxy flow, PluginManager.api_call(), plugin_manager on AgentEngine"
provides:
  - "Skill bundling: YAML files copied from plugin dirs to ~/.tamux/skills/plugins/{name}/"
  - "Command registry: manifest commands registered as /pluginname.command entries"
  - "Command dispatch: agent_loop augments plugin command messages with system hints"
  - "PluginListCommands IPC message for listing all registered plugin commands"
affects: [19-02-plan, 20-gmail-calendar-validation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Plugin command namespacing: /pluginname.commandname per PSKL-05"
    - "LLM augmentation for command dispatch: system hint injection rather than LLM bypass"

key-files:
  created:
    - crates/amux-daemon/src/plugin/skills.rs
    - crates/amux-daemon/src/plugin/commands.rs
  modified:
    - crates/amux-daemon/src/plugin/mod.rs
    - crates/amux-protocol/src/messages.rs
    - crates/amux-daemon/src/server.rs
    - crates/amux-daemon/src/agent/engine.rs
    - crates/amux-daemon/src/agent/agent_loop.rs

key-decisions:
  - "LLM augmentation over bypass: plugin commands inject system hints so LLM naturally uses plugin API tool, preserving the agent's tool-calling loop"
  - "OnceLock for plugin_manager on AgentEngine: set after both are constructed in server.rs, avoids circular dependency"
  - "Flat skill directory: skill files flattened to filename only in target dir (no subdirectory nesting)"

patterns-established:
  - "Plugin command pattern: /pluginname.commandname parsed by parse_plugin_command()"
  - "System hint injection: try_augment_plugin_command() injects context for LLM before inference"

requirements-completed: [PSKL-01, PSKL-02, PSKL-03, PSKL-04, PSKL-05]

# Metrics
duration: 4min
completed: 2026-03-24
---

# Phase 19 Plan 01: Plugin Skills & Commands Summary

**Plugin skill bundling (YAML copy/remove lifecycle), command registry with /pluginname.command dispatch, and agent_loop LLM augmentation for plugin commands**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-24T23:33:56Z
- **Completed:** 2026-03-24T23:37:53Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Skill bundling module: install_bundled_skills copies YAML files on register, remove_bundled_skills cleans up on unregister
- Command registry: PluginCommandRegistry with rebuild_from_plugins, resolve, list_all, and is_empty methods
- PluginListCommands/PluginCommandsResult IPC messages wired through server.rs
- Agent loop intercepts /pluginname.command messages and injects system hints for LLM
- parse_plugin_command pure function with 6 unit tests
- 91 plugin tests + 43 protocol tests all passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Skill bundling module and command registry with IPC** - `e2c4e44` (test) + `5798a2e` (feat) -- TDD RED/GREEN from prior execution
2. **Task 2: Agent command interception and dispatch in agent_loop** - `5fcab77` (test) + `c61f9c4` (feat)

_Note: Task 1 was already completed in a prior execution. Task 2 was implemented in this session._

## Files Created/Modified
- `crates/amux-daemon/src/plugin/skills.rs` - Skill copy/remove functions for plugin lifecycle
- `crates/amux-daemon/src/plugin/commands.rs` - Command registry and dispatch for plugin slash commands
- `crates/amux-daemon/src/plugin/mod.rs` - PluginManager integration with skills, commands, and rebuild_command_registry
- `crates/amux-protocol/src/messages.rs` - PluginListCommands, PluginCommandsResult, PluginCommandInfo wire types
- `crates/amux-daemon/src/server.rs` - PluginListCommands IPC handler
- `crates/amux-daemon/src/agent/engine.rs` - plugin_manager OnceLock field on AgentEngine
- `crates/amux-daemon/src/agent/agent_loop.rs` - parse_plugin_command, try_augment_plugin_command, command injection in send_message_inner

## Decisions Made
- LLM augmentation over bypass: plugin commands inject system hints so LLM naturally uses plugin API tool, preserving the agent's tool-calling loop
- OnceLock for plugin_manager on AgentEngine: set after both are constructed in server.rs, avoids circular dependency
- Flat skill directory: skill files flattened to filename only in target dir (no subdirectory nesting)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plugin skill bundling and command dispatch complete
- Ready for Plan 19-02 (if it exists) or Phase 20 Gmail/Calendar validation
- All workspace compiles clean, 91 plugin + 43 protocol + 6 agent_loop tests passing

## Self-Check: PASSED

All 7 files verified present. All 4 commit hashes verified in git log.

---
*Phase: 19-plugin-skills-commands*
*Completed: 2026-03-24*
