---
phase: 17-api-proxy-layer
plan: 01
subsystem: api
tags: [ssrf, rate-limiter, handlebars, template-engine, ipc, plugin-api]

# Dependency graph
requires:
  - phase: 14-plugin-manifest
    provides: PluginManager, manifest types (ApiSection, EndpointDef, RateLimitDef)
  - phase: 16-plugin-settings
    provides: Plugin settings persistence (get_settings returns key/value/secret tuples)
provides:
  - SSRF IP validation module (ssrf.rs) blocking internal/private IP ranges
  - Token bucket rate limiter (rate_limiter.rs) with per-plugin tracking
  - PluginApiError enum with 8 structured error variants
  - RenderedRequest struct for template output
  - Handlebars template engine (template.rs) with strict mode and 5 custom helpers
  - PluginApiCall/PluginApiCallResult IPC message variants
affects: [17-api-proxy-layer, 18-oauth2-flow, 20-validation-plugin]

# Tech tracking
tech-stack:
  added: [handlebars 6.4, percent-encoding 2, url 2]
  patterns: [token-bucket rate limiting, SSRF pre-resolution validation, handlebars strict mode templates, spawn_blocking + timeout for template rendering]

key-files:
  created:
    - crates/amux-daemon/src/plugin/ssrf.rs
    - crates/amux-daemon/src/plugin/rate_limiter.rs
    - crates/amux-daemon/src/plugin/api_proxy.rs
    - crates/amux-daemon/src/plugin/template.rs
  modified:
    - crates/amux-daemon/src/plugin/mod.rs
    - crates/amux-daemon/Cargo.toml
    - crates/amux-protocol/src/messages.rs
    - crates/amux-daemon/src/server.rs

key-decisions:
  - "url crate added as direct dependency for SSRF URL parsing (reqwest pulls it transitively but ssrf.rs uses it directly)"
  - "Handlebars helpers use manual HelperDef impl (not handlebars_helper! macro) for full control over parameter handling"
  - "render_request is async wrapping spawn_blocking + 1s timeout for template isolation"
  - "PluginApiCall server handler returns not_implemented placeholder until Plan 17-02 wires full orchestration"
  - "serde skip_serializing_if removed from PluginApiCallResult.error_type for bincode compatibility"

patterns-established:
  - "SSRF validation: DNS pre-resolution via tokio::net::lookup_host before checking IP against blocklist"
  - "Token bucket rate limiter: per-plugin via RateLimiterMap with lazy bucket creation"
  - "Template isolation: spawn_blocking + tokio::time::timeout(1s) for rendering safety"

requirements-completed: [APRX-02, APRX-04, APRX-05, APRX-06, APRX-07]

# Metrics
duration: 9min
completed: 2026-03-24
---

# Phase 17 Plan 01: API Proxy Foundation Summary

**SSRF validation, token bucket rate limiter, Handlebars template engine with 5 helpers, PluginApiError enum, and IPC protocol messages for plugin API proxy**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-24T23:06:46Z
- **Completed:** 2026-03-24T23:16:04Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- SSRF validation blocks all RFC 1918, loopback, link-local, cloud metadata (169.254.169.254), IPv6 ULA, and IPv4-mapped IPv6 addresses before HTTP requests
- Token bucket rate limiter enforces configurable RPM with time-based refill, tracking per-plugin via RateLimiterMap
- Handlebars template engine with strict mode and 5 custom helpers (urlencode, json, default, truncate, join) for request/response transformation
- PluginApiError enum with 8 variants providing structured, actionable error messages
- IPC protocol messages PluginApiCall/PluginApiCallResult for client-daemon API call routing
- 42 new tests across 4 modules all passing (26 ssrf/rate_limiter/api_proxy + 13 template + 3 protocol)

## Task Commits

Each task was committed atomically:

1. **Task 1: SSRF validation, rate limiter, and PluginApiError modules** - `f96cc9b` (feat)
2. **Task 2: Handlebars template engine, IPC protocol messages, and Cargo dependency** - `0cffc10` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/plugin/ssrf.rs` - SSRF IP validation with DNS pre-resolution and IPv4-mapped IPv6 support
- `crates/amux-daemon/src/plugin/rate_limiter.rs` - TokenBucket with time-based refill and RateLimiterMap for per-plugin tracking
- `crates/amux-daemon/src/plugin/api_proxy.rs` - PluginApiError enum (8 variants) and RenderedRequest struct
- `crates/amux-daemon/src/plugin/template.rs` - Handlebars registry with strict mode, 5 helpers, build_context, render_request, render_response
- `crates/amux-daemon/src/plugin/mod.rs` - Module declarations and PluginApiError re-export
- `crates/amux-daemon/Cargo.toml` - Added handlebars 6.4, percent-encoding 2, url 2
- `crates/amux-protocol/src/messages.rs` - PluginApiCall/PluginApiCallResult IPC variants with bincode roundtrip tests
- `crates/amux-daemon/src/server.rs` - Stub handler for PluginApiCall returning not_implemented

## Decisions Made
- Used `url` crate as direct dependency for SSRF URL parsing rather than relying on reqwest's transitive dep
- Handlebars helpers implemented as manual HelperDef closures rather than handlebars_helper! macro for full control over parameter handling and error messages
- render_request wraps template rendering in spawn_blocking + 1s tokio timeout for safety
- PluginApiCall server handler returns not_implemented placeholder until Plan 17-02 wires the full api_call() orchestration
- Removed serde skip_serializing_if from PluginApiCallResult.error_type field because bincode (non-self-describing format) doesn't support it

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added url crate dependency**
- **Found during:** Task 1 (SSRF module)
- **Issue:** ssrf.rs uses url::Url::parse() but url crate was not a direct dependency
- **Fix:** Added `url = "2"` to daemon Cargo.toml
- **Files modified:** crates/amux-daemon/Cargo.toml
- **Verification:** Compiles and tests pass
- **Committed in:** f96cc9b (Task 1 commit)

**2. [Rule 1 - Bug] Fixed lifetime issues in Handlebars helper functions**
- **Found during:** Task 2 (template module)
- **Issue:** Temporary value from `.to_string()` freed while borrowed in helper closures
- **Fix:** Bound the string to a named variable before borrowing
- **Files modified:** crates/amux-daemon/src/plugin/template.rs
- **Verification:** Compiles without lifetime errors, all tests pass
- **Committed in:** 0cffc10 (Task 2 commit)

**3. [Rule 1 - Bug] Removed serde skip_serializing_if from bincode-serialized field**
- **Found during:** Task 2 (IPC protocol messages)
- **Issue:** bincode roundtrip test failed with UnexpectedEof because skip_serializing_if is incompatible with bincode's non-self-describing format
- **Fix:** Removed `#[serde(default, skip_serializing_if = "Option::is_none")]` from error_type field
- **Files modified:** crates/amux-protocol/src/messages.rs
- **Verification:** All 41 protocol tests pass including 3 new roundtrip tests
- **Committed in:** 0cffc10 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All auto-fixes necessary for correct compilation and bincode compatibility. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
- `crates/amux-daemon/src/server.rs`: PluginApiCall handler returns "not_implemented" placeholder. This is intentional -- Plan 17-02 will wire the full api_call() orchestration.

## Next Phase Readiness
- All four foundation modules (ssrf, rate_limiter, template, api_proxy) are independently testable and ready for composition
- Plan 17-02 can compose these modules into the full api_call() orchestration function
- IPC messages are in place for client-daemon API call routing

## Self-Check: PASSED

All files created, all commits found, all tests passing.

---
*Phase: 17-api-proxy-layer*
*Completed: 2026-03-24*
