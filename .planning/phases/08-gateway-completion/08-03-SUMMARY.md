---
phase: 08-gateway-completion
plan: 03
subsystem: gateway
tags: [rate-limiting, chunking, markdown, thread-replies, slack, discord, telegram, heartbeat, unreplied-detection]

# Dependency graph
requires:
  - phase: 08-gateway-completion
    provides: TokenBucket rate limiters, markdown format converters, chunk_message, GatewayState with health/reply_contexts/rate limiters
provides:
  - Thread-aware gateway send tools with format conversion, rate limiting, and chunking
  - Real unreplied gateway message detection in heartbeat check
  - last_response_at tracking for all outgoing gateway messages
  - Telegram MarkdownV2 with plain text fallback on parse errors
affects: [08-04]

# Tech tracking
tech-stack:
  added: []
  patterns: [rate-limit-then-send gateway pattern, format-chunk-send pipeline, MarkdownV2-with-fallback, last_incoming_at vs last_response_at unreplied detection]

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/tool_executor.rs
    - crates/amux-daemon/src/agent/heartbeat_checks.rs

key-decisions:
  - "Combined Tasks 1 and 2 into a single commit since format conversion, rate limiting, chunking, and thread replies are interleaved in the same function rewrites"
  - "Thread context auto-injection uses try_lock on gateway_state to avoid blocking when lock is contended"
  - "Discord message_reference uses fail_if_not_exists: false to avoid errors when referenced message is deleted"
  - "Telegram MarkdownV2 fallback re-chunks the plain text version (not just re-sends original chunks) since escaping changes length"
  - "Unreplied detection uses threshold_hours to avoid flagging messages that just arrived (gives agent time to respond)"

patterns-established:
  - "Rate-limit-then-send: acquire token from GatewayState rate limiter, sleep if needed, then send"
  - "Format-chunk-send pipeline: convert markdown -> chunk at platform limit -> send each chunk"
  - "Thread context auto-injection with agent override: check args first, then fall back to reply_contexts map"
  - "last_response_at tracking: update after every successful send for heartbeat unreplied detection"

requirements-completed: [GATE-01, GATE-02, GATE-03, GATE-06]

# Metrics
duration: 17min
completed: 2026-03-23
---

# Phase 08 Plan 03: Gateway Send Enhancement Summary

**Thread-aware, format-converted, rate-limited, chunked gateway send tools with real unreplied message detection via last_incoming_at vs last_response_at comparison**

## Performance

- **Duration:** 17 min
- **Started:** 2026-03-23T22:08:40Z
- **Completed:** 2026-03-23T22:25:33Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- All three gateway send tools (Slack, Discord, Telegram) now check rate limiters, convert markdown to platform format, chunk long messages, auto-inject thread context, and track last_response_at
- Telegram uses MarkdownV2 parse_mode with automatic plain text fallback when Telegram API rejects MarkdownV2 entities
- check_unreplied_messages replaced from Phase 2 stub (items_found: 0) to real detection using last_incoming_at vs last_response_at comparison with configurable threshold_hours

## Task Commits

Each task was committed atomically:

1. **Tasks 1+2: Rate limiting, chunking, format conversion, and thread-aware replies for gateway send tools** - `dcab424` (feat)
2. **Task 3: Real unreplied gateway message detection in heartbeat check** - `f6668f8` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/tool_executor.rs` - Enhanced send_slack_message, send_discord_message, send_telegram_message with rate limiting (TokenBucket), message chunking (chunk_message), format conversion (markdown_to_*), thread-aware replies (thread_ts, message_reference, reply_to_message_id), MarkdownV2 fallback, and last_response_at tracking
- `crates/amux-daemon/src/agent/heartbeat_checks.rs` - Replaced Phase 2 stub with real unreplied detection comparing last_incoming_at vs last_response_at per channel, with severity escalation and threshold_hours filtering

## Decisions Made
- Combined Tasks 1 and 2 into a single commit since the format conversion, rate limiting, chunking, and thread reply changes are deeply intertwined in the same function bodies -- separating them into two commits would require artificial code splitting
- Used `try_lock` (non-blocking) for thread context lookups from reply_contexts to avoid potential deadlock when gateway_state lock is already held by the poll loop
- Discord message_reference includes `fail_if_not_exists: false` to gracefully handle cases where the referenced message was deleted
- Telegram MarkdownV2 fallback re-chunks from the original message text (via markdown_to_plain) rather than reusing MarkdownV2 chunks, since escaping changes character counts
- Unreplied detection severity escalation: Low at threshold, Medium at 2x threshold, High at 4x threshold -- matching the same pattern used by check_stale_todos

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed Discord poll_discord missing thread_context field in IncomingMessage**
- **Found during:** Task 1 (reading gateway.rs to understand structure)
- **Issue:** The parallel Plan 02 agent had added thread_context to IncomingMessage struct but poll_discord was still missing the field in its struct literal, which would cause compilation errors
- **Fix:** Added thread_context population to poll_discord (capturing discord message ID)
- **Files modified:** crates/amux-daemon/src/agent/gateway.rs (fixed by Plan 02 agent during parallel execution)
- **Verification:** cargo check passes
- **Committed in:** by parallel Plan 02 agent

**2. [Rule 1 - Bug] Updated heartbeat test assertion for new summary text**
- **Found during:** Task 3 (running heartbeat_checks tests)
- **Issue:** Existing test `heartbeat_checks_unreplied_empty_gateway` expected "No active gateway" in summary but new implementation outputs "No unreplied gateway"
- **Fix:** Updated assertion from "No active gateway" to "No unreplied gateway"
- **Files modified:** crates/amux-daemon/src/agent/heartbeat_checks.rs
- **Verification:** All 9 heartbeat_checks tests pass
- **Committed in:** f6668f8 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking from parallel execution, 1 test update)
**Impact on plan:** Both auto-fixes necessary for compilation and test correctness. No scope creep.

## Issues Encountered
- Plan 02 executing in parallel modified gateway.rs concurrently, causing file-modified-since-read errors when attempting to fix compilation issues. Resolved by focusing on tool_executor.rs first and letting Plan 02 complete its gateway.rs changes.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all send tools are fully wired with rate limiting, format conversion, chunking, and thread-aware replies. The heartbeat check uses real data from GatewayState.

## Next Phase Readiness
- All gateway send tools enhanced with production-quality features
- Unreplied detection available for heartbeat cycle to surface missed messages
- last_response_at tracking feeds into Plan 04 monitoring dashboards if applicable
- Thread context auto-injection enables natural conversational replies across all platforms

---
*Phase: 08-gateway-completion*
*Completed: 2026-03-23*
