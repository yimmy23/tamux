---
phase: 10-progressive-ux
plan: 01
subsystem: agent-engine
tags: [capability-tier, progressive-disclosure, operator-model, protocol, setup-wizard, agent-bridge]

# Dependency graph
requires:
  - phase: 01-production-foundation
    provides: "AgentEngine, HistoryStore, circuit breakers, operator model"
provides:
  - "CapabilityTier enum with 4-tier progressive disclosure system"
  - "resolve_tier() pure function driven by operator model signals"
  - "TierFeatureFlags mapping tiers to visible feature sets"
  - "DisclosureQueue for one-per-session feature draining"
  - "AgentStatusQuery/AgentSetTierOverride protocol messages"
  - "AgentTierChanged/AgentStatusResponse protocol messages"
  - "Engine methods: compute_current_tier, get_status_snapshot, set_tier_override"
  - "Agent bridge GetStatus/SetTierOverride commands with JSON forwarding"
  - "Setup wizard self-assessment question persisted to tier config"
affects: [10-02, 10-03, 10-04, 10-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure function tier resolution with override > behavioral > self-assessment elevation"
    - "impl AgentEngine blocks in capability_tier.rs for engine integration"
    - "TierConfig in AgentConfig with serde defaults for backward compat"

key-files:
  created:
    - "crates/amux-daemon/src/agent/capability_tier.rs"
  modified:
    - "crates/amux-protocol/src/messages.rs"
    - "crates/amux-daemon/src/server.rs"
    - "crates/amux-daemon/src/agent/mod.rs"
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/agent/operator_model.rs"
    - "crates/amux-cli/src/setup_wizard.rs"
    - "crates/amux-cli/src/client.rs"

key-decisions:
  - "CapabilityTier visibility pub(crate) on module to allow server.rs access for tier parsing"
  - "AgentSetTierOverride sends no explicit success response (matches existing AgentSetConfigItem pattern); TierChanged event broadcast on actual change"
  - "Gateway statuses built as manual JSON from PlatformHealthState fields since struct lacks Serialize derive"
  - "AuditEntryRow serialized to lightweight JSON (id, timestamp, action_type, summary) since it lacks Serialize derive"
  - "persist_config() is fire-and-forget (returns unit); tier override relies on in-memory config update"

patterns-established:
  - "Tier resolution: pure function resolve_tier() takes TierSignals, testable without engine"
  - "Engine extension via impl AgentEngine in capability_tier.rs (same pattern as operator_model.rs)"
  - "Agent bridge forwarding: add command variant, dispatch to ClientMessage, add DaemonMessage match arm"

requirements-completed: [PRUX-01]

# Metrics
duration: 14min
completed: 2026-03-24
---

# Phase 10 Plan 01: Capability Tier Foundation Summary

**CapabilityTier enum with 4-tier progressive disclosure, pure resolve_tier() function, protocol messages, engine wiring, agent bridge forwarding, and setup wizard self-assessment**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-24T08:12:20Z
- **Completed:** 2026-03-24T08:26:20Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- CapabilityTier enum (Newcomer/Familiar/PowerUser/Expert) with PartialOrd ordering and pure resolve_tier() function
- TierFeatureFlags maps tiers to 6 boolean feature visibility controls per D-04
- DisclosureQueue supports one-per-session feature draining per D-13
- Protocol messages: AgentStatusQuery, AgentSetTierOverride, AgentTierChanged, AgentStatusResponse
- Server dispatches tier messages to engine; engine computes tier from operator model signals
- Agent bridge forwards GetStatus/SetTierOverride commands and relays TierChanged/StatusResponse to Electron
- Setup wizard asks "How familiar are you with AI agents?" and persists self-assessment to tier config
- 13 unit tests covering all tier resolution rules and edge cases

## Task Commits

Each task was committed atomically:

1. **Task 1: Create capability_tier.rs with CapabilityTier enum, resolve_tier(), and tier config types** - `7aa0409` (feat)
2. **Task 2: Add protocol messages, server handlers, engine wiring, agent bridge commands, and setup wizard self-assessment** - `3771a98` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/capability_tier.rs` - CapabilityTier enum, resolve_tier(), TierFeatureFlags, DisclosureQueue, TierConfig, AgentEngine tier methods
- `crates/amux-daemon/src/agent/mod.rs` - Register capability_tier module with pub(crate) visibility
- `crates/amux-daemon/src/agent/types.rs` - TierConfig in AgentConfig, TierChanged in AgentEvent
- `crates/amux-daemon/src/agent/operator_model.rs` - unique_tools_seen and goal_runs_completed fields
- `crates/amux-protocol/src/messages.rs` - AgentStatusQuery, AgentSetTierOverride, AgentTierChanged, AgentStatusResponse
- `crates/amux-daemon/src/server.rs` - Server dispatch for tier query and override
- `crates/amux-cli/src/setup_wizard.rs` - Self-assessment question, capability_tier field, tier config in JSON output
- `crates/amux-cli/src/client.rs` - GetStatus/SetTierOverride bridge commands, TierChanged/StatusResponse forwarding

## Decisions Made
- CapabilityTier module is `pub(crate)` to allow server.rs to import CapabilityTier for tier string parsing
- AgentSetTierOverride silently succeeds (no explicit response); TierChanged broadcast notifies all clients
- Gateway health serialized manually since PlatformHealthState lacks Serialize derive
- AuditEntryRow serialized to lightweight JSON (4 fields) since the struct lacks Serialize derive
- persist_config() is fire-and-forget; engine relies on in-memory RwLock<AgentConfig> for immediate effect

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] now_millis() location mismatch**
- **Found during:** Task 2 (engine wiring)
- **Issue:** Plan referenced `super::types::now_millis()` but function is in `task_prompt.rs` (glob-used as `super::now_millis()`)
- **Fix:** Changed to `super::now_millis()`
- **Files modified:** crates/amux-daemon/src/agent/capability_tier.rs
- **Committed in:** 3771a98

**2. [Rule 3 - Blocking] DaemonMessage::Ok does not exist**
- **Found during:** Task 2 (server handler)
- **Issue:** Plan specified `DaemonMessage::Ok` response but the enum has no Ok variant
- **Fix:** Used no-response-on-success pattern (matching existing AgentSetConfigItem) with DaemonMessage::Error on parse failure
- **Files modified:** crates/amux-daemon/src/server.rs
- **Committed in:** 3771a98

**3. [Rule 3 - Blocking] AuditEntryRow lacks Serialize**
- **Found during:** Task 2 (get_status_snapshot)
- **Issue:** AuditEntryRow is `#[derive(Debug, Clone)]` only, cannot serde_json::to_string directly
- **Fix:** Built lightweight JSON manually from individual fields
- **Files modified:** crates/amux-daemon/src/agent/capability_tier.rs
- **Committed in:** 3771a98

**4. [Rule 3 - Blocking] PlatformHealthState lacks Serialize**
- **Found during:** Task 2 (get_status_snapshot gateway statuses)
- **Issue:** Plan assumed a platform_statuses field; GatewayState has individual health structs without Serialize
- **Fix:** Built JSON manually from individual platform health state fields
- **Files modified:** crates/amux-daemon/src/agent/capability_tier.rs
- **Committed in:** 3771a98

**5. [Rule 3 - Blocking] persist_config() returns unit, not Result**
- **Found during:** Task 2 (set_tier_override)
- **Issue:** Plan used `if let Err(e)` pattern but persist_config() returns `()`
- **Fix:** Changed to direct call `self.persist_config().await`
- **Files modified:** crates/amux-daemon/src/agent/capability_tier.rs
- **Committed in:** 3771a98

---

**Total deviations:** 5 auto-fixed (5 blocking - all Rule 3)
**Impact on plan:** All auto-fixes necessary to match actual codebase APIs. No scope creep. All plan objectives achieved.

## Issues Encountered
None beyond the deviations documented above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all data sources are wired to real daemon state (operator model, goal runs, circuit breakers, gateway health, audit trail).

## Next Phase Readiness
- Tier foundation complete; all Phase 10 plans 02-05 can now build on CapabilityTier, TierFeatureFlags, and protocol messages
- Frontend tier store (Plan 02) can consume AgentStatusResponse and AgentTierChanged via existing agent bridge
- TUI tier integration (Plan 03) can compute tier locally using the same resolve_tier() signals
- Concierge onboarding (Plan 04) can use DisclosureQueue for progressive feature revelation

## Self-Check: PASSED

- FOUND: crates/amux-daemon/src/agent/capability_tier.rs
- FOUND: .planning/phases/10-progressive-ux/10-01-SUMMARY.md
- FOUND: commit 7aa0409 (Task 1)
- FOUND: commit 3771a98 (Task 2)

---
*Phase: 10-progressive-ux*
*Completed: 2026-03-24*
