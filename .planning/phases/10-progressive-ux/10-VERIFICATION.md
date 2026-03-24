---
phase: 10-progressive-ux
verified: 2026-03-24T08:55:54Z
status: gaps_found
score: 9/14 must-haves verified
gaps:
  - truth: "Onboarding content varies by tier: Newcomer gets walkthrough, Expert gets minimal greeting"
    status: failed
    reason: "deliver_onboarding() is defined and substantive but never called — orphaned. The existing RequestConciergeWelcome path calls generate_welcome() which is NOT tier-adapted. The tier-adapted onboarding is dead code."
    artifacts:
      - path: "crates/amux-daemon/src/agent/concierge.rs"
        issue: "deliver_onboarding() defined at line 688 but has zero callers in the entire codebase"
    missing:
      - "Wire deliver_onboarding() into the RequestConciergeWelcome handler in server.rs, checking onboarding_completed flag to gate first-run vs returning user"
      - "Alternatively, call deliver_onboarding() from on_client_connected() when config.tier.onboarding_completed == false"

  - truth: "When tier changes, concierge announces the transition via in-chat message (D-12)"
    status: failed
    reason: "announce_tier_transition() is defined but never called. check_tier_change() broadcasts TierChanged event (visual UI update) but does NOT call announce_tier_transition() for the in-chat concierge message."
    artifacts:
      - path: "crates/amux-daemon/src/agent/concierge.rs"
        issue: "announce_tier_transition() defined at line 804 but has zero callers"
      - path: "crates/amux-daemon/src/agent/capability_tier.rs"
        issue: "check_tier_change() at line 399 broadcasts AgentEvent::TierChanged but does not call self.concierge.announce_tier_transition()"
    missing:
      - "In check_tier_change(), after broadcasting TierChanged event, call self.concierge.announce_tier_transition(&previous_tier_str, &new_tier_str).await"

  - truth: "User can see recent autonomous actions (last 3-5) with explanations in all three clients"
    status: partial
    reason: "CLI (tamux status) displays recent_actions. statusStore.ts stores recentActions. But StatusBar.tsx does not render recentActions from the store, and TUI sidebar has no recent actions display. Only CLI achieves this truth."
    artifacts:
      - path: "frontend/src/components/StatusBar.tsx"
        issue: "Imports and reads activity/providerHealth from statusStore but recentActions field from store is never rendered"
      - path: "crates/amux-tui/src/widgets/sidebar.rs"
        issue: "Shows agent_status_line (activity + tier) but no recent_actions from AgentStatusResponse"
    missing:
      - "StatusBar.tsx: render recentActions from useStatusStore — e.g., a tooltip or expandable section showing last 3 autonomous actions with explanations"
      - "TUI sidebar: add a recent actions section showing last 3 entries from statusStore or a dedicated wire path"

  - truth: "Zero occurrences of (window as any).tamux or (window as any).amux remain in frontend/src/"
    status: partial
    reason: "All (window as any).tamux/.amux unsafe casts were eliminated. However TypeScript compilation fails with 3 pre-existing errors in agentTools.ts (useSettingsStore not imported). These errors exist in the codebase before phase 10 but PRUX-05 truth requires clean TypeScript compilation."
    artifacts:
      - path: "frontend/src/lib/agentTools.ts"
        issue: "Line 1968, 1978, 1979: TS2304 Cannot find name 'useSettingsStore' — missing import that pre-existed phase 10"
    missing:
      - "Add missing import: import { useSettingsStore } from './settingsStore'; to agentTools.ts"
---

# Phase 10: Progressive UX — Verification Report

**Phase Goal:** The interface reveals depth as the user grows — newcomers see simplicity, power users see the full system
**Verified:** 2026-03-24T08:55:54Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | Daemon resolves CapabilityTier from operator model signals | VERIFIED | `resolve_tier()` reads session_count, unique_tools_seen, goal_runs_completed, risk_tolerance; 13 unit tests pass |
| 2  | User override always takes precedence over behavioral tier | VERIFIED | `resolve_tier()` returns `user_override` immediately if set; test confirms |
| 3  | Setup wizard asks experience level and persists it | VERIFIED | `setup_wizard.rs` line 278: "How familiar are you with AI agents?"; `capability_tier` in SetupResult |
| 4  | TierChanged event emitted to all clients when tier changes | VERIFIED | `check_tier_change()` broadcasts `AgentEvent::TierChanged`; server forwards to all subscribed clients |
| 5  | Zero (window as any).tamux/.amux unsafe casts in frontend/src/ | VERIFIED | Only remaining instances are typed `window.tamux` (not casts) in CDUIApp.tsx and SetupOnboardingPanel.tsx |
| 6  | getBridge() returns AmuxBridge | null from shared bridge.ts | VERIFIED | `frontend/src/lib/bridge.ts` — 9 lines, exported, used across all modified files |
| 7  | Onboarding content varies by tier (Newcomer/Expert/etc) | FAILED | `deliver_onboarding()` has 4 tier-adapted messages but is NEVER CALLED — orphaned function |
| 8  | When tier changes, concierge announces in-chat (D-12) | FAILED | `announce_tier_transition()` defined but never called; `check_tier_change()` doesn't invoke it |
| 9  | Feature disclosure drains one item per session (D-13) | VERIFIED | `deliver_next_disclosure()` called from `gateway_loop.rs` line 330; `DisclosureQueue.next_disclosure()` enforces one-per-session |
| 10 | New concierge actions handled in TUI and Electron | VERIFIED | TUI `app.rs` lines 407-430: all 4 actions handled; `ConciergeToast.tsx` lines 39-55: all 4 actions handled |
| 11 | Newcomer tier hides advanced features; Expert sees all | VERIFIED | `TierGatedSection` in SettingsPanel wraps 5 sections; TUI sidebar guards 5 sections with `tier.show_*` booleans |
| 12 | Hidden features appear as collapsed sections, not invisible | VERIFIED | `TierGatedSection.tsx` renders expandable `<button>` with tier label when tier not met |
| 13 | User can see agent activity state in all three clients | VERIFIED | Electron StatusBar renders activity from statusStore; TUI sidebar renders `agent_status_line`; CLI `tamux status` prints activity |
| 14 | User can see recent autonomous actions in all three clients | FAILED | CLI shows recent_actions; Electron statusStore stores them but StatusBar doesn't render them; TUI shows no recent actions |

**Score:** 10/14 truths verified (4 failed, 1 partial counted as failed)

---

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/amux-daemon/src/agent/capability_tier.rs` | CapabilityTier enum, resolve_tier(), TierFeatureFlags, DisclosureQueue, TierConfig | VERIFIED | All exports present; `pub(super)` visibility on enum and functions; 13 tests pass |
| `crates/amux-protocol/src/messages.rs` | AgentStatusQuery, AgentSetTierOverride, AgentTierChanged, AgentStatusResponse | VERIFIED | All 4 messages present at lines 544, 547, 943, 950 |
| `crates/amux-cli/src/setup_wizard.rs` | Self-assessment question, capability_tier in SetupResult | VERIFIED | "How familiar are you with AI agents?" at line 278; `capability_tier` field at line 22 |
| `crates/amux-cli/src/client.rs` | GetStatus, SetTierOverride bridge commands | VERIFIED | Lines 201-202; dispatch at 1339-1343; forwarding at 1469/1484 |
| `frontend/src/lib/bridge.ts` | getBridge() shared accessor | VERIFIED | 9 lines, exports `getBridge(): AmuxBridge | null` |
| `crates/amux-daemon/src/agent/concierge.rs` | deliver_onboarding(), onboarding_template_fallback(), announce_tier_transition() | STUB/ORPHANED | Methods exist and are substantive but have zero callers — wiring is broken |
| `frontend/src/lib/tierStore.ts` | useTierStore, hydrateTierStore, CapabilityTier | VERIFIED | All exports present; hydrates from daemon config |
| `frontend/src/components/base-components/TierGatedSection.tsx` | TierGatedSection component | VERIFIED | Exports `TierGatedSection`, reads from useTierStore, collapses below-tier sections |
| `crates/amux-tui/src/state/tier.rs` | TierState with from_tier(), on_tier_changed() | VERIFIED | 6 unit tests pass; registered in state/mod.rs; field in app.rs at line 95 |
| `frontend/src/components/SettingsPanel.tsx` | 5 TierGatedSection wrappers + agentSetTierOverride | VERIFIED | 11 TierGatedSection usages; agentSetTierOverride at line 65 |
| `frontend/src/lib/statusStore.ts` | useStatusStore, hydrateStatusStore, AgentActivityState | VERIFIED | All exports present; polls via agentGetStatus every 10s |
| `frontend/src/components/StatusBar.tsx` | Activity state, resource indicators, tier badge | PARTIAL | Activity, provider health, tier badge rendered; recentActions and gatewayStatuses from store not rendered |
| `crates/amux-tui/src/widgets/sidebar.rs` | Activity state display + tier-gated sections | VERIFIED | `agent_status_line()` at line 290; 5 tier gates at lines 272-284 |
| `crates/amux-cli/src/main.rs` | Status subcommand | VERIFIED | `Commands::Status` at line 112; handler at line 556 with full output |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `capability_tier.rs` | `operator_model.rs` | `resolve_tier` reads session_count, goal_runs_completed, unique_tools_seen | VERIFIED | Lines 256-258 in compute_current_tier() |
| `engine.rs` | `capability_tier.rs` | `AgentEngine` holds `disclosure_queue: RwLock<DisclosureQueue>` | VERIFIED | engine.rs line 126; initialized at line 222 |
| `server.rs` | `messages.rs` | Dispatches AgentSetTierOverride and AgentStatusQuery | VERIFIED | server.rs lines 2791, 2796 |
| `client.rs` | `messages.rs` | Agent bridge sends GetStatus/SetTierOverride, forwards TierChanged/StatusResponse | VERIFIED | Lines 1339-1343; 1469-1484 |
| `concierge.rs` | `capability_tier.rs` | deliver_onboarding() reads tier-adapted content | ORPHANED | deliver_onboarding() exists but is never called by any code path |
| `gateway_loop.rs` | `capability_tier.rs` | Calls check_tier_change() on heartbeat cycle | VERIFIED | gateway_loop.rs lines 322, 330 |
| `concierge.rs` | `check_tier_change()` | announce_tier_transition() called on tier change | NOT_WIRED | check_tier_change() does NOT call announce_tier_transition() |
| `tierStore.ts` | `bridge.ts` | getBridge().agentGetConfig for hydration | VERIFIED | tierStore.ts line 58 |
| `TierGatedSection.tsx` | `tierStore.ts` | useTierStore reads tierOrdinal | VERIFIED | TierGatedSection.tsx line 13 |
| `App.tsx` | `tierStore.ts` | tier_changed event handler calls setTier | VERIFIED | App.tsx lines 205, 210 |
| `SettingsPanel.tsx` | `preload.cjs` | agentSetTierOverride → IPC → main.cjs | VERIFIED | All three wired: preload line 269, main.cjs line 4290, bridge.d.ts line 325 |
| `statusStore.ts` | `main.cjs` | agentGetStatus IPC chain | VERIFIED | preload line 267, main.cjs line 4268, bridge.d.ts line 313 |

---

## Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|--------------|--------|-------------------|--------|
| `StatusBar.tsx` | `activity` | `useStatusStore(s => s.activity)` → `pollStatus()` → `bridge.agentGetStatus()` → daemon | Yes (daemon assembles from engine state) | FLOWING |
| `StatusBar.tsx` | `recentActions` | `useStatusStore(s => s.recentActions)` — store has field | No — StatusBar never reads/renders `recentActions` | HOLLOW_PROP |
| `TierGatedSection.tsx` | `tierOrdinal` | `useTierStore(s => s.tierOrdinal)` → `hydrateTierStore()` → daemon config | Yes | FLOWING |
| `TUI sidebar` | `agent_activity` | `self.agent_activity` field updated from agent events (writing/reasoning/tool) | Yes (from live agent events, not status snapshot) | FLOWING |
| `concierge.rs` `deliver_onboarding()` | `tier` parameter | Would read from compute_current_tier() | N/A — function is never called | DISCONNECTED |

---

## Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| CapabilityTier unit tests pass | `cargo test -p tamux-daemon -- capability_tier::tests -q` | 13 passed, 0 failed | PASS |
| TUI tier state unit tests pass | `cargo test -p tamux-tui -- state::tier::tests -q` | 6 passed, 0 failed | PASS |
| TypeScript compiles without errors | `cd frontend && npx tsc --noEmit` | 3 errors in agentTools.ts (pre-existing: useSettingsStore not imported) | FAIL |
| Zero unsafe (window as any) bridge casts | `grep -rn "(window as any)\.(tamux|amux)" frontend/src/ | grep -v .d.ts | wc -l` | 0 (comment in bridge.ts excluded) | PASS |
| TierGatedSection wraps >= 5 sections in SettingsPanel | `grep -c "TierGatedSection" SettingsPanel.tsx` | 11 | PASS |
| Agent bridge forwards GetStatus/SetTierOverride | `grep -n "GetStatus\|SetTierOverride" client.rs` | Lines 201, 202, 1339-1343 | PASS |
| deliver_onboarding() has callers | `grep -rn "deliver_onboarding" crates/` | 1 result (definition only) | FAIL |
| announce_tier_transition() has callers | `grep -rn "announce_tier_transition" crates/` | 1 result (definition only) | FAIL |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| PRUX-01 | 10-01 | Capability tiers driven by operator model: Newcomer → Familiar → Power User → Expert | SATISFIED | resolve_tier() reads operator model signals; 13 tests verify all tier computations |
| PRUX-02 | 10-03 | Tier transitions announced naturally via concierge | BLOCKED | announce_tier_transition() is defined but never called; check_tier_change() does not invoke it |
| PRUX-03 | 10-04 | New users see simplified interface; advanced capabilities revealed as usage grows | SATISFIED | 5 TierGatedSection wrappers in SettingsPanel; TUI sidebar guards 5 sections; TierGatedSection shows collapsed sections not invisible |
| PRUX-04 | 10-03 | Concierge onboarding: guided first experience with hands-on examples | BLOCKED | deliver_onboarding() has 4 tier-adapted messages but zero callers; existing RequestConciergeWelcome path calls generate_welcome() which is NOT tier-adapted |
| PRUX-05 | 10-02 | Typed getBridge() helper replaces all unsafe (window as any) casts | SATISFIED | Zero (window as any) casts remain; getBridge() canonical in bridge.ts; 3 pre-existing TS errors in agentTools.ts unrelated to cast elimination |
| PRUX-06 | 10-05 | Consistent status visibility across TUI, Electron, and CLI | PARTIAL | Activity state visible across all 3 clients. Recent autonomous actions only in CLI. Gateway per-platform health from statusStore not rendered in Electron. |

---

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/amux-daemon/src/agent/concierge.rs` | 688 | `deliver_onboarding()` defined but has zero callers | Blocker | PRUX-04 tier-adapted onboarding never triggers |
| `crates/amux-daemon/src/agent/concierge.rs` | 804 | `announce_tier_transition()` defined but has zero callers | Blocker | PRUX-02 tier transition in-chat announcement never triggers |
| `frontend/src/lib/agentTools.ts` | 1968,1978,1979 | `useSettingsStore` used without import — pre-existing TypeScript error | Warning | Prevents clean TypeScript compilation (pre-existing, not introduced by phase 10) |
| `frontend/src/components/StatusBar.tsx` | — | `recentActions` stored in statusStore but never rendered | Warning | PRUX-06 recent actions not visible in Electron |

---

## Human Verification Required

### 1. Tier Progression Feel

**Test:** Start tamux as a new user. Send messages in several sessions. Use goal runs and tools. Observe when tier changes.
**Expected:** At session 5+ with 3+ tools used, tier should auto-promote to Familiar. Further usage promotes to PowerUser/Expert.
**Why human:** Operator model signals accumulate over real usage sessions — cannot verify in a unit test.

### 2. Concierge Welcome Quality

**Test:** Complete setup wizard with "Just getting started" (Newcomer). Observe the welcome message.
**Expected:** A warm, encouraging message in simple language with a "Send a message" action button.
**Why human:** The actual welcome currently uses generate_welcome() (not tier-adapted). This needs a human to confirm whether the current welcome is acceptable for newcomers OR if the tier-adapted deliver_onboarding() gap is user-visible.

### 3. TierGatedSection Expand/Collapse UX

**Test:** Set tier to "newcomer" in SettingsPanel. Navigate to Behavior, Goal Runs, Sub-Agents, Gateway, Memory tabs.
**Expected:** Each tab shows a collapsed section with a label and tier requirement, expandable on click.
**Why human:** Visual rendering and accessibility of collapsed sections requires human inspection.

---

## Gaps Summary

Phase 10 is largely successful — the core infrastructure (capability tier system, operator model integration, tier gating in Electron and TUI, bridge unification, status visibility) is built and wired. However, two critical behavioral features are defined but not connected to their trigger paths:

**Gap 1 — Onboarding not triggered (PRUX-04):** `deliver_onboarding()` was built with 4 tier-adapted messages and an LLM fallback, but the function has zero callers. When a user first connects, the server calls `generate_welcome()` (a generic welcome) rather than `deliver_onboarding()`. The fix is 3-5 lines: in `server.rs`, the `AgentRequestConciergeWelcome` handler should check `config.tier.onboarding_completed` and call `deliver_onboarding(tier)` for first-time users.

**Gap 2 — Tier announcement not triggered (PRUX-02):** `announce_tier_transition()` was built with a natural concierge voice, but `check_tier_change()` does not call it. The TierChanged event is broadcast (clients update their UI tier badge), but there is no in-chat message saying "You've been promoted to Power User." The fix is 1 line: in `check_tier_change()`, after broadcasting the event, call `self.concierge.announce_tier_transition(&previous_tier_str, &new_tier_str).await`.

**Gap 3 — Recent actions not visible in Electron/TUI (PRUX-06):** The statusStore accumulates `recentActions` from the daemon but `StatusBar.tsx` doesn't render them. The CLI shows them fully. This is a display gap — the data pipeline is correct but the rendering end is incomplete.

**Gap 4 — TypeScript compilation errors (PRUX-05):** 3 pre-existing errors in `agentTools.ts` prevent clean `tsc --noEmit`. These were not introduced by phase 10 but they block the PRUX-05 truth "TypeScript compilation produces no errors."

---

_Verified: 2026-03-24T08:55:54Z_
_Verifier: Claude (gsd-verifier)_
