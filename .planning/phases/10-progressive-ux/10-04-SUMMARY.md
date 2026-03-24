---
phase: 10-progressive-ux
plan: 04
subsystem: ui
tags: [zustand, react, ratatui, tier-gating, progressive-disclosure, ipc]

# Dependency graph
requires:
  - phase: 10-01
    provides: "Protocol messages for tier (AgentStatusQuery, AgentSetTierOverride, AgentTierChanged)"
  - phase: 10-02
    provides: "Typed bridge helper getBridge() and AmuxBridge type declarations"
provides:
  - "Zustand tierStore with hydration and feature flag computation"
  - "TierGatedSection React component for collapsed below-tier features (D-05)"
  - "TUI TierState with pre-computed visibility booleans for efficient rendering"
  - "Tier override IPC wiring: preload.cjs, main.cjs handler, bridge.d.ts type"
  - "Tier selector dropdown in SettingsPanel for user override (D-03)"
  - "5 D-04 tier-gated sections wrapped in SettingsPanel"
  - "TUI sidebar tier-gated placeholders for hidden sections"
affects: [10-05-progressive-ux]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Tier gating via TierGatedSection wrapper component (Electron)"
    - "Pre-computed boolean flags for per-frame tier checks (TUI)"
    - "Tier hydration from daemon config (override > self_assessment > newcomer)"

key-files:
  created:
    - frontend/src/lib/tierStore.ts
    - frontend/src/components/base-components/TierGatedSection.tsx
    - crates/amux-tui/src/state/tier.rs
  modified:
    - frontend/src/main.tsx
    - frontend/src/App.tsx
    - frontend/src/components/SettingsPanel.tsx
    - frontend/electron/main.cjs
    - frontend/electron/preload.cjs
    - frontend/src/types/amux-bridge.d.ts
    - crates/amux-tui/src/state/mod.rs
    - crates/amux-tui/src/app.rs
    - crates/amux-tui/src/app/events.rs
    - crates/amux-tui/src/app/config_io.rs
    - crates/amux-tui/src/app/rendering.rs
    - crates/amux-tui/src/widgets/sidebar.rs
    - crates/amux-tui/src/client.rs

key-decisions:
  - "Tier gating wraps tab content in SettingsPanel (behavior, agent, gateway, subagents, about) rather than individual sub-sections"
  - "TUI tier placeholders render as dimmed lines at sidebar bottom rather than replacing existing sections"
  - "Tier event handling accepts both tier_changed and tier-changed event types for forward/backward compat with agent bridge"
  - "TUI hydrates tier from config JSON on first load, then updates via TierChanged events"

patterns-established:
  - "TierGatedSection: wrap any section with requiredTier prop for progressive disclosure"
  - "TierState pre-computed booleans: compute once on tier change, check per frame without logic"

requirements-completed: [PRUX-03]

# Metrics
duration: 9min
completed: 2026-03-24
---

# Phase 10 Plan 04: Client-Side Tier Gating Summary

**Zustand tierStore with feature flags, TierGatedSection component for D-05 collapsed sections, TUI TierState with pre-computed visibility booleans, tier override IPC wiring, and 5 D-04 sections gated in both Electron and TUI**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-24T08:30:42Z
- **Completed:** 2026-03-24T08:39:53Z
- **Tasks:** 2
- **Files modified:** 16

## Accomplishments
- Zustand tierStore hydrates current tier from daemon config on startup with override > self_assessment > newcomer priority
- TierGatedSection component collapses below-tier features as expandable sections with tier label (D-05)
- All 5 D-04 sections wrapped in SettingsPanel: behavior (task queue), agent (goal runs), gateway, sub-agents, about (memory/learning)
- Tier override selector dropdown in SettingsPanel calls agentSetTierOverride via fully wired IPC (preload + main.cjs + bridge.d.ts)
- App.tsx listens for tier_changed daemon events and updates store in real-time
- TUI TierState has 6 pre-computed visibility booleans updated once on tier change (not per render frame)
- TUI sidebar renders dimmed placeholders for tier-locked sections with required tier label
- 6 unit tests verify tier-to-visibility mappings across all 4 tiers plus unknown

## Task Commits

Each task was committed atomically:

1. **Task 1: Create Zustand tierStore, TierGatedSection, tier override IPC, wrap 5 D-04 sections** - `f2e0107` (feat)
2. **Task 2: Create TUI TierState with pre-computed visibility flags and sidebar gating** - `59691a1` (feat)

## Files Created/Modified
- `frontend/src/lib/tierStore.ts` - Zustand store for capability tier with hydration and feature flag computation
- `frontend/src/components/base-components/TierGatedSection.tsx` - Reusable tier-gated section wrapper component
- `frontend/src/main.tsx` - Added hydrateTierStore() to bootstrap sequence
- `frontend/src/App.tsx` - Added tier_changed event handler in onAgentEvent listener
- `frontend/src/components/SettingsPanel.tsx` - Added tier selector dropdown and wrapped 5 D-04 sections
- `frontend/electron/preload.cjs` - Added agentSetTierOverride bridge method
- `frontend/electron/main.cjs` - Added agent-set-tier-override IPC handler
- `frontend/src/types/amux-bridge.d.ts` - Added agentSetTierOverride type declaration
- `crates/amux-tui/src/state/tier.rs` - TierState struct with pre-computed visibility booleans and 6 tests
- `crates/amux-tui/src/state/mod.rs` - Registered tier module
- `crates/amux-tui/src/app.rs` - Added pub tier: TierState field to TuiModel
- `crates/amux-tui/src/app/events.rs` - Handle TierChanged ClientEvent
- `crates/amux-tui/src/app/config_io.rs` - Hydrate tier from daemon config JSON
- `crates/amux-tui/src/app/rendering.rs` - Pass tier to sidebar render
- `crates/amux-tui/src/widgets/sidebar.rs` - Tier-gated sections with dimmed placeholders
- `crates/amux-tui/src/client.rs` - Added TierChanged ClientEvent variant and parser

## Decisions Made
- Tier gating wraps tab content in SettingsPanel (behavior, agent, gateway, subagents, about) rather than individual sub-sections within tabs, since each tab is already a cohesive feature section
- TUI tier placeholders render as dimmed lines at the sidebar bottom rather than replacing existing sections, preserving layout stability
- Tier event handling accepts both tier_changed and tier-changed formats for compatibility with the agent bridge which emits "tier-changed"
- TUI hydrates tier from config JSON on first AgentConfigRaw event, then updates via TierChanged events for real-time changes

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## Known Stubs
None - all tier gating is fully wired to daemon config and IPC.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Tier gating infrastructure is complete for both Electron and TUI
- Plan 10-05 can build on this to add usage-based tier progression
- All 5 D-04 sections gated and ready for future refinement of per-section visibility

---
*Phase: 10-progressive-ux*
*Completed: 2026-03-24*
