---
phase: 17-api-proxy-layer
plan: 02
subsystem: api
tags: [reqwest, handlebars, ssrf, rate-limiting, ipc, tool-executor, plugin-proxy]

# Dependency graph
requires:
  - phase: 17-api-proxy-layer/01
    provides: "SSRF validation, rate limiter, template engine, PluginApiError types, RenderedRequest struct, PluginApiCall/PluginApiCallResult protocol messages"
provides:
  - "PluginManager.api_call() full orchestration (lookup -> enabled check -> rate limit -> settings -> template -> SSRF -> HTTP -> response template)"
  - "execute_request() HTTP execution with timeout, error handling, and JSON parsing"
  - "plugin_api_call agent tool definition and execute_tool handler"
  - "PluginApiCall IPC handler in server.rs with structured error responses"
  - "AgentEngine.plugin_manager OnceLock field for cross-module access"
affects: [18-oauth2-flow, 19-plugin-commands, 20-validation-gmail-calendar]

# Tech tracking
tech-stack:
  added: []
  patterns: ["OnceLock for late-initialized Arc field on AgentEngine", "early-return ToolResult for missing args (non-Result return type)"]

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/plugin/api_proxy.rs"
    - "crates/amux-daemon/src/plugin/mod.rs"
    - "crates/amux-daemon/src/agent/engine.rs"
    - "crates/amux-daemon/src/agent/tool_executor.rs"
    - "crates/amux-daemon/src/server.rs"
    - "crates/amux-daemon/src/agent/heartbeat_checks.rs"

key-decisions:
  - "OnceLock for plugin_manager field on AgentEngine to avoid runtime locking overhead while supporting post-construction initialization"
  - "Early-return ToolResult pattern for missing args in plugin_api_call handler (execute_tool returns ToolResult, not Result)"
  - "plugin_api_call tool always available in get_available_tools() -- PluginManager handles disabled/missing checks internally"

patterns-established:
  - "OnceLock<Arc<T>> pattern for late-initialized shared references on AgentEngine"
  - "PluginApiError error_type string classification for IPC error responses"

requirements-completed: [APRX-01, APRX-03]

# Metrics
duration: 8min
completed: 2026-03-25
---

# Phase 17 Plan 02: API Proxy Wiring Summary

**End-to-end plugin API proxy wired into AgentEngine tool executor and IPC server with full orchestration flow: lookup, rate limit, template render, SSRF check, HTTP request, response transformation**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-24T23:24:41Z
- **Completed:** 2026-03-24T23:33:00Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- AgentEngine has plugin_manager field (OnceLock) wired from server.rs after construction
- Agent can invoke plugin_api_call tool to call any enabled plugin API endpoint through the full proxy flow
- IPC clients can send PluginApiCall message and receive PluginApiCallResult with structured error types
- PluginManager.api_call() orchestrates complete flow: plugin lookup, enabled check, rate limit, settings fetch, template render, SSRF validation, HTTP execution, response transformation
- All 76 core plugin tests pass, 43 protocol tests pass, 59 heartbeat tests pass (no regressions)

## Task Commits

Each task was committed atomically:

1. **Task 1+2: HTTP execution, PluginManager orchestration, AgentEngine wiring, tool executor, IPC handler** - `270abda` (feat)

**Plan metadata:** pending

## Files Created/Modified
- `crates/amux-daemon/src/plugin/api_proxy.rs` - execute_request() for HTTP calls with timeout/error handling (from Wave 1)
- `crates/amux-daemon/src/plugin/mod.rs` - PluginManager.api_call() orchestration, http_client/rate_limiters/template_registry fields (from Wave 1)
- `crates/amux-daemon/src/agent/engine.rs` - plugin_manager OnceLock field on AgentEngine struct + constructor
- `crates/amux-daemon/src/agent/tool_executor.rs` - plugin_api_call tool definition + execute_tool handler
- `crates/amux-daemon/src/server.rs` - PluginApiCall IPC handler with error type classification (from Wave 1)
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Added plugin_manager field to test helper

## Decisions Made
- Used `std::sync::OnceLock<Arc<PluginManager>>` for the AgentEngine field since AgentEngine is wrapped in Arc before plugin_manager is available -- OnceLock avoids runtime lock overhead while allowing post-construction initialization
- plugin_api_call tool handler uses early-return ToolResult pattern for missing arguments since execute_tool() returns ToolResult (not Result), making `?` operator incompatible
- Tool definition added unconditionally (not gated on config.tools) since PluginManager handles disabled/missing checks internally with proper error messages

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed `?` operator in execute_tool non-Result function**
- **Found during:** Task 2 (tool executor wiring)
- **Issue:** Plan template used `?` operator on `ok_or_else()` in the `plugin_api_call` match arm, but `execute_tool()` returns `ToolResult` (not `Result`), causing compilation failure
- **Fix:** Replaced `?` with explicit `match` + early `return ToolResult` for missing argument cases
- **Files modified:** crates/amux-daemon/src/agent/tool_executor.rs
- **Verification:** `cargo build -p tamux-daemon` compiles clean
- **Committed in:** 270abda

**2. [Rule 1 - Bug] Added missing plugin_manager field to test helper**
- **Found during:** Task 2 (test compilation)
- **Issue:** `heartbeat_checks.rs` test helper constructs AgentEngine directly and was missing the new `plugin_manager` field, causing test compilation failure
- **Fix:** Added `plugin_manager: std::sync::OnceLock::new()` to the test struct initializer
- **Files modified:** crates/amux-daemon/src/agent/heartbeat_checks.rs
- **Verification:** `cargo test -p tamux-daemon -- heartbeat` -- all 59 tests pass
- **Committed in:** 270abda

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both fixes necessary for compilation. No scope creep.

## Issues Encountered
- Wave 1 (Plan 17-01) implemented most of the Plan 02 scope (execute_request, api_call, server.rs handler) ahead of schedule, so Plan 02 only needed the AgentEngine wiring and tool executor additions

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Full API proxy flow working end-to-end for both agent tools and IPC clients
- Ready for Phase 18 (OAuth2 flow support) to add authentication flows
- Ready for Phase 19 (Plugin commands) to add slash command registration
- Ready for Phase 20 (Gmail/Calendar validation) to prove the full plugin lifecycle

## Self-Check: PASSED

All files verified present. Commit 270abda verified in git log.

---
*Phase: 17-api-proxy-layer*
*Completed: 2026-03-25*
