---
phase: 01-production-foundation
plan: 03
subsystem: agent
tags: [circuit-breaker, resilience, llm-client, per-provider, fail-fast, event-forwarding]

requires:
  - "Async HistoryStore with tokio_rusqlite::Connection (from Plan 01)"
  - "Single shared Arc<HistoryStore> created in server.rs and distributed to all components (from Plan 02)"
provides:
  - "Per-provider circuit breakers gating all 6 LLM call sites"
  - "AgentEvent::ProviderCircuitOpen and ProviderCircuitRecovered event variants"
  - "CircuitBreakerRegistry with dynamic provider creation"
  - "Server-side forwarding of circuit breaker events to all connected clients"
  - "Graceful degradation with alternative provider suggestion (D-14)"
affects: []

tech-stack:
  added: []
  patterns: [per-provider circuit breaker registry via Arc<CircuitBreakerRegistry>, check-before/record-after LLM call pattern, Arc sharing between AgentEngine and ConciergeEngine]

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/circuit_breaker.rs
    - crates/amux-daemon/src/agent/types.rs
    - crates/amux-daemon/src/agent/engine.rs
    - crates/amux-daemon/src/agent/mod.rs
    - crates/amux-daemon/src/agent/agent_loop.rs
    - crates/amux-daemon/src/agent/goal_llm.rs
    - crates/amux-daemon/src/agent/concierge.rs
    - crates/amux-daemon/src/agent/memory_flush.rs
    - crates/amux-daemon/src/server.rs

key-decisions:
  - "Approach B (wrap at call sites) chosen over Approach A (modify send_completion_request) to keep streaming logic clean and give each call site access to event_tx and circuit_breakers"
  - "CircuitBreakerRegistry wrapped in Arc for sharing between AgentEngine and ConciergeEngine"
  - "ConciergeEngine receives its own circuit breaker helper methods (check/record) since it is a standalone struct, not part of AgentEngine"
  - "Memory flush gracefully skips on circuit breaker open rather than failing the parent operation"

patterns-established:
  - "Check-before/record-after pattern: call check_circuit_breaker() before LLM call, record_llm_outcome() after stream completes"
  - "Per-provider isolation: each provider string key gets its own independent CircuitBreaker instance"
  - "Dynamic provider creation: CircuitBreakerRegistry.get() creates a default breaker for unknown providers on-demand"

requirements-completed: [FOUN-04]

duration: 10min
completed: 2026-03-23
---

# Phase 01 Plan 03: Circuit Breaker Wiring Summary

**Wired per-provider circuit breakers into all 6 LLM call sites with AgentEvent notifications, server event forwarding, and graceful degradation with alternative provider suggestion**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-23T02:02:55Z
- **Completed:** 2026-03-23T02:13:35Z
- **Tasks:** 2 completed
- **Files modified:** 9

## Accomplishments

- Activated the existing CircuitBreaker (328 lines, 13 tests) from dead code into the live LLM call path
- Created CircuitBreakerRegistry for per-provider breaker isolation with dynamic provider creation
- Wired circuit breaker check before and outcome recording after all 6 send_completion_request call sites (agent_loop, 2x goal_llm, 2x concierge, memory_flush)
- Added ProviderCircuitOpen and ProviderCircuitRecovered AgentEvent variants forwarded to all connected clients
- Added suggest_alternative_provider for graceful degradation when a provider is down
- Verified protocol compatibility via serde roundtrip test (JSON serialization)
- All 505 daemon tests pass, 16 circuit breaker tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add AgentEvent variants and create CircuitBreakerRegistry** - `bc76f27` (feat)
2. **Task 2: Wire circuit breakers into send_completion_request call path** - `ec53c36` (feat)

## Files Created/Modified

- `crates/amux-daemon/src/agent/circuit_breaker.rs` - Added CircuitBreakerRegistry struct with per-provider management, updated module doc, added isolation test
- `crates/amux-daemon/src/agent/types.rs` - Added ProviderCircuitOpen and ProviderCircuitRecovered AgentEvent variants, added serde roundtrip test
- `crates/amux-daemon/src/agent/engine.rs` - Added circuit_breakers field (Arc<CircuitBreakerRegistry>), check_circuit_breaker/record_llm_outcome/suggest_alternative_provider methods
- `crates/amux-daemon/src/agent/mod.rs` - Removed #[allow(dead_code)] from circuit_breaker module
- `crates/amux-daemon/src/agent/agent_loop.rs` - Wired circuit breaker check before LLM call, outcome recording on success/error
- `crates/amux-daemon/src/agent/goal_llm.rs` - Wired circuit breaker check before both goal LLM calls (raw and JSON), outcome recording
- `crates/amux-daemon/src/agent/concierge.rs` - Added CircuitBreakerRegistry to ConciergeEngine, wired circuit breaker for both welcome and triage calls
- `crates/amux-daemon/src/agent/memory_flush.rs` - Wired circuit breaker check before memory flush LLM call, graceful skip on open
- `crates/amux-daemon/src/server.rs` - Updated should_forward_agent_event to broadcast circuit breaker events to all clients

## Decisions Made

- **Approach B (wrap at call sites):** Chose to add circuit breaker checks at each of the 6 call sites rather than modifying send_completion_request itself. This keeps the streaming function pure and gives each call site access to event_tx for error reporting.
- **Arc<CircuitBreakerRegistry> for sharing:** Since ConciergeEngine is a standalone struct (not part of AgentEngine), the registry is wrapped in Arc and passed to both. This ensures a single source of truth for provider health.
- **Memory flush graceful skip:** When the circuit breaker is open, memory_flush returns Ok(false) instead of propagating the error, so the parent agent loop continues normally without the pre-compaction flush.
- **ConciergeEngine gets its own check/record methods:** Duplicated the circuit breaker helper logic in ConciergeEngine rather than adding a complex trait, since the helpers are simple and the concierge is intentionally decoupled from AgentEngine.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ConciergeEngine constructor needed CircuitBreakerRegistry parameter**
- **Found during:** Task 2
- **Issue:** ConciergeEngine is a standalone struct constructed in engine.rs. Adding circuit breaker support required changing its constructor signature.
- **Fix:** Added `circuit_breakers: Arc<CircuitBreakerRegistry>` parameter to ConciergeEngine::new(), updated the test constructor in concierge.rs tests.
- **Files modified:** concierge.rs, engine.rs
- **Commit:** ec53c36

**2. [Rule 1 - Bug] suggest_alternative_provider visibility**
- **Found during:** Task 2
- **Issue:** The method was defined as private (`async fn`) but called from agent_loop.rs which is a sibling module.
- **Fix:** Changed to `pub(super) async fn`.
- **Files modified:** engine.rs
- **Commit:** ec53c36

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for compilation. No scope creep.

## Known Stubs

None - all functionality is fully wired.

## Notes

- Frontend clients (agentClient.ts) and TUI (wire.rs) need to handle the new AgentEvent variants. Since events are JSON-serialized and clients should have a fallback for unknown event types, this is backward-compatible. Follow-up UI work to display circuit breaker status can be done in a future phase.
- The circuit breaker defaults (5 failures to trip, 2 successes to recover, 30s open duration) are hardcoded in CircuitBreaker::default(). Making these configurable via AgentConfig is deferred to a future phase if needed.

## Next Phase Readiness

- Phase 01 (production-foundation) is complete: async SQLite, shared HistoryStore, circuit breaker wiring
- All 505 daemon tests pass
- Ready for Phase 02 (heartbeat system) or any subsequent phase

## Self-Check: PASSED

---
*Phase: 01-production-foundation*
*Completed: 2026-03-23*
