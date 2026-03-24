---
phase: 10-progressive-ux
plan: 03
subsystem: agent-engine
tags: [concierge, onboarding, tier-transition, disclosure-queue, heartbeat, tui, electron]

# Dependency graph
requires:
  - phase: 10-progressive-ux
    plan: 01
    provides: "CapabilityTier enum, TierConfig, DisclosureQueue, resolve_tier()"
  - phase: 10-progressive-ux
    plan: 02
    provides: "getBridge() typed accessor for Electron frontend"
provides:
  - "Tier-adapted onboarding with LLM + static fallback in concierge"
  - "Onboarding actions: start_goal_run, dismiss_welcome, focus_chat, open_settings"
  - "Tier transition announcements via concierge (D-12)"
  - "Feature disclosure queue draining one-per-session (D-13)"
  - "check_tier_change() on heartbeat cycle detecting tier promotions"
  - "TUI action handlers for all 4 new concierge action strings"
  - "Electron ConciergeToast handlers for all 4 new concierge action strings"
affects: [10-04, 10-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "LLM-powered onboarding with static template fallback (Pitfall 5)"
    - "ConciergeActionType enum extended with 4 new action variants"
    - "Heartbeat-integrated tier change detection and disclosure draining"

key-files:
  created: []
  modified:
    - "crates/amux-daemon/src/agent/concierge.rs"
    - "crates/amux-daemon/src/agent/capability_tier.rs"
    - "crates/amux-daemon/src/agent/types.rs"
    - "crates/amux-daemon/src/agent/engine.rs"
    - "crates/amux-daemon/src/agent/gateway_loop.rs"
    - "crates/amux-tui/src/app.rs"
    - "frontend/src/components/ConciergeToast.tsx"

key-decisions:
  - "ConciergeActionType extended with StartGoalRun/DismissWelcome/FocusChat/OpenSettings variants rather than using raw strings for type safety"
  - "TUI open_settings action opens Auth tab (first settings tab) since there is no dedicated tier settings view"
  - "Onboarding uses ConciergeDetailLevel::ContextSummary for LLM-generated content, Minimal for disclosure"
  - "Tier change detection runs every heartbeat cycle (after heartbeat checks, before cron recompute)"
  - "populate_disclosure_queue receives mutable config reference but currently only uses the disclosure queue field"

patterns-established:
  - "Onboarding flow: deliver_onboarding() tries LLM, falls back to static template per tier"
  - "Action type pattern: extend ConciergeActionType enum + add match arms in TUI and Electron"
  - "Disclosure queue populated on tier change, drained one-per-session during heartbeat"

requirements-completed: [PRUX-02, PRUX-04]

# Metrics
duration: 7min
completed: 2026-03-24
---

# Phase 10 Plan 03: Concierge Onboarding and Tier Transitions Summary

**Tier-adapted concierge onboarding with LLM + static fallback, tier transition announcements, feature disclosure queue draining, and new action handlers in TUI and Electron**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-24T08:31:56Z
- **Completed:** 2026-03-24T08:39:46Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Tier-adapted onboarding via `deliver_onboarding()` with 4 distinct messages per tier (D-08)
- LLM-powered onboarding with `generate_onboarding_llm()` using tier-specific system prompts
- Static template fallback via `onboarding_template_fallback()` for when LLM fails (Pitfall 5)
- Onboarding state tracking via `onboarding_completed` flag in TierConfig (D-09)
- Tier transition announcements via `announce_tier_transition()` with natural concierge voice (D-12)
- Feature disclosure queue with `deliver_next_disclosure()` draining one feature per session (D-13)
- `check_tier_change()` detects tier promotions each heartbeat cycle and populates disclosure queue
- `tier_disclosure_features()` defines 6 features across 3 tiers (Familiar: 3, PowerUser: 2, Expert: 1)
- `disclosure_queue` field added to AgentEngine for persistent queue state
- 4 new `ConciergeActionType` variants: StartGoalRun, DismissWelcome, FocusChat, OpenSettings
- TUI `execute_concierge_action` handles all 4 new action strings with appropriate navigation
- Electron `ConciergeToast` handles all 4 new action strings with store-based navigation

## Task Commits

Each task was committed atomically:

1. **Task 1: Add tier-adapted onboarding to concierge with LLM + static fallback, and wire action handlers in TUI and Electron** - `602dfc8` (feat)
2. **Task 2: Add tier transition announcements and disclosure queue draining to heartbeat** - `fac414b` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/concierge.rs` - deliver_onboarding, generate_onboarding_llm, onboarding_actions, announce_tier_transition, deliver_next_disclosure, onboarding_template_fallback, onboarding_system_prompt
- `crates/amux-daemon/src/agent/capability_tier.rs` - check_tier_change, populate_disclosure_queue, tier_disclosure_features, onboarding_completed/last_known_tier in TierConfig
- `crates/amux-daemon/src/agent/types.rs` - StartGoalRun/DismissWelcome/FocusChat/OpenSettings in ConciergeActionType
- `crates/amux-daemon/src/agent/engine.rs` - disclosure_queue field on AgentEngine
- `crates/amux-daemon/src/agent/gateway_loop.rs` - check_tier_change and deliver_next_disclosure wired into heartbeat cycle
- `crates/amux-tui/src/app.rs` - start_goal_run, dismiss_welcome, focus_chat, open_settings match arms in execute_concierge_action
- `frontend/src/components/ConciergeToast.tsx` - start_goal_run, dismiss_welcome, focus_chat, open_settings handlers with workspace store integration

## Decisions Made
- Extended ConciergeActionType enum with 4 new typed variants rather than using raw strings for compile-time safety
- TUI open_settings navigates to Settings modal Auth tab (first tab) since no dedicated tier settings view exists
- Onboarding messages use ConciergeDetailLevel::ContextSummary for main content, Minimal for feature disclosures
- Tier change detection runs on every heartbeat cycle regardless of quiet hours since tier changes are cheap to detect
- Disclosure queue lives on AgentEngine (not ConciergeEngine) for shared access from heartbeat

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ConciergeActionType is enum not string**
- **Found during:** Task 1
- **Issue:** Plan assumed ConciergeAction uses string action_type, but it uses a typed enum ConciergeActionType with serde rename_all
- **Fix:** Added 4 new enum variants (StartGoalRun, DismissWelcome, FocusChat, OpenSettings) to ConciergeActionType
- **Files modified:** crates/amux-daemon/src/agent/types.rs
- **Committed in:** 602dfc8

**2. [Rule 3 - Blocking] ConciergeWelcome event has detail_level field**
- **Found during:** Task 1
- **Issue:** Plan's deliver_onboarding omitted the required detail_level field on ConciergeWelcome
- **Fix:** Added detail_level: ConciergeDetailLevel::ContextSummary to all ConciergeWelcome emits
- **Files modified:** crates/amux-daemon/src/agent/concierge.rs
- **Committed in:** 602dfc8

**3. [Rule 3 - Blocking] No MainPaneView::Settings or FocusArea::Settings in TUI**
- **Found during:** Task 1
- **Issue:** Plan assumed settings view is a MainPaneView variant, but TUI uses modal overlay via open_settings_tab()
- **Fix:** Used self.open_settings_tab(SettingsTab::Auth) for the open_settings action
- **Files modified:** crates/amux-tui/src/app.rs
- **Committed in:** 602dfc8

**4. [Rule 3 - Blocking] Electron settings opened via workspaceStore not settingsStore**
- **Found during:** Task 1
- **Issue:** Plan assumed settingsStore has an open() method, but settings panel uses workspaceStore.toggleSettings()
- **Fix:** Used useWorkspaceStore toggleSettings/settingsOpen for the open_settings action
- **Files modified:** frontend/src/components/ConciergeToast.tsx
- **Committed in:** 602dfc8

---

**Total deviations:** 4 auto-fixed (all Rule 3 - blocking)
**Impact on plan:** All auto-fixes necessary to match actual codebase APIs. No scope creep. All plan objectives achieved.

## Issues Encountered
- Pre-existing TypeScript errors in agentTools.ts (useSettingsStore not found) are not caused by this plan

## Known Stubs
None - all functions are fully implemented with real LLM integration and static fallbacks.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Onboarding flow complete; Plan 04/05 can build on deliver_onboarding for first-run integration
- Tier transitions wired into heartbeat; feature disclosure drains automatically
- Action handlers in TUI and Electron ready for end-to-end testing

## Self-Check: PASSED

- FOUND: crates/amux-daemon/src/agent/concierge.rs
- FOUND: crates/amux-daemon/src/agent/capability_tier.rs
- FOUND: crates/amux-daemon/src/agent/engine.rs
- FOUND: crates/amux-daemon/src/agent/gateway_loop.rs
- FOUND: crates/amux-tui/src/app.rs
- FOUND: frontend/src/components/ConciergeToast.tsx
- FOUND: .planning/phases/10-progressive-ux/10-03-SUMMARY.md
- FOUND: commit 602dfc8 (Task 1)
- FOUND: commit fac414b (Task 2)

---
*Phase: 10-progressive-ux*
*Completed: 2026-03-24*
