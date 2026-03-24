# Phase 10: Progressive UX - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

The interface reveals depth as the user grows — newcomers see simplicity, power users see the full system. This phase delivers capability tiers driven by the operator model, concierge onboarding with hands-on first goal run, tier-adapted feature disclosure, typed bridge helper, and consistent status visibility across all clients (TUI, Electron, CLI). No new agent capabilities — only UX layering over existing features.

</domain>

<decisions>
## Implementation Decisions

### Capability Tier System
- **D-01:** Hybrid tier triggering: operator model signals (session count, tool diversity, goal run usage, risk tolerance) drive automatic promotion, PLUS user self-assessment in setup wizard. Both inputs feed the tier assignment.
- **D-02:** Four tiers: Newcomer, Familiar, Power User, Expert. Each maps to a set of visible features.
- **D-03:** Users can override tier both directions in settings — manually promote or demote. Power users who reinstall shouldn't be stuck in Newcomer. Newcomers overwhelmed by auto-promotion can step back.
- **D-04:** Feature visibility by tier:
  - **Newcomer:** Basic chat, simple tasks, status bar, concierge. Hidden (collapsed): goal runs, task queue, gateway config, memory controls, sub-agents.
  - **Familiar:** + Goal runs, task queue, gateway/automation config. Hidden (collapsed): sub-agent management, memory/learning controls.
  - **Power User:** + Sub-agent management, advanced settings. Hidden (collapsed): memory/learning controls.
  - **Expert:** Everything visible, no collapsed sections.
- **D-05:** Hidden features appear as collapsed sections (not invisible, not grayed-out). Users can expand to peek but won't be overwhelmed. Balances discovery with simplicity.
- **D-06:** Setup wizard asks experience level via natural language: "How familiar are you with AI agents?" → "Just getting started" / "I've used chatbots" / "I run automations" / "I build agent systems" → maps to Newcomer/Familiar/Power User/Expert.

### Concierge Onboarding
- **D-07:** After setup wizard completes, concierge delivers a guided first goal run: greets user, explains what tamux can do at their tier level, then walks through a hands-on example. This is the TTFV moment — "first success without configuration."
- **D-08:** Onboarding is tier-adapted:
  - **Newcomer:** "Here's what I can do" + simple first task walkthrough
  - **Familiar:** "Here's what's new since chatbots" + goal run demonstration
  - **Power User:** "Here's your workspace" + advanced features overview
  - **Expert:** "Config loaded. Ready." — minimal, no walkthrough
- **D-09:** Onboarding is always skippable, no follow-up. One-shot offer. If dismissed, never comes back unless user explicitly asks. Maximum respect for user time.

### Typed Bridge Helper (PRUX-05)
- **D-10:** Single `getBridge(): AmuxBridge | null` function replaces all 39 `(window as any).tamux ?? (window as any).amux` casts. Returns null when not in Electron. Type-safe, single import. Uses existing `AmuxBridge` interface from `amux-bridge.d.ts`.

### Status Visibility Parity (PRUX-06)
- **D-11:** Four status categories must be consistent across TUI, Electron, and CLI:
  1. **Agent activity state:** idle, thinking, executing tool, waiting for approval, running goal
  2. **Current task context:** active thread/task/goal, current step, progress if applicable
  3. **Resource indicators:** provider health (circuit breaker), gateway connections, memory usage, token budget
  4. **Recent action summary:** last 3-5 autonomous actions with one-line explanations (leverages Phase 3 audit feed)

### Feature Disclosure Strategy
- **D-12:** Tier transitions announced via in-chat concierge message, not notification popups or settings highlights. Natural concierge voice: "You've been using goal runs for a week — did you know you can also run sub-agents? Here's how."
- **D-13:** Features disclosed one at a time, spread over days. When promoted to a new tier, don't dump 5 features at once. Introduce one per session over the next few days via concierge. Prevents overwhelm.

### Claude's Discretion
- Exact operator model thresholds for tier promotion (how many sessions, what tool diversity score, etc.)
- Concierge welcome message content and tone per tier
- Status widget layout details per client
- Which specific features map to which collapsed sections in each tier
- How `getBridge()` handles edge cases (e.g., bridge available but daemon disconnected)
- Implementation of the "one feature per session" disclosure queue

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### UX Strategy
- `.planning/good_ux.md` — Comprehensive UX analysis: personas (R/A/D), capability matrix, autonomy ↔ control trade-off, TTFV metrics, gap analysis. PRIMARY reference for all UX decisions in this phase.

### Existing Infrastructure (Daemon)
- `crates/amux-daemon/src/agent/operator_model.rs` — OperatorModel with CognitiveStyle, RiskFingerprint, SessionRhythm, AttentionTopology. Source of behavioral signals for tier assignment.
- `crates/amux-daemon/src/agent/concierge.rs` — ConciergeEngine with 4 detail levels (Minimal → DailyBriefing), welcome generation, action buttons. Foundation for onboarding.
- `crates/amux-daemon/src/agent/types.rs` §OperatorModelConfig (L1131-1152) — Config flags for operator model features.
- `crates/amux-daemon/src/agent/types.rs` §ConciergeConfig (L1744-1767) — Config for concierge engine (detail_level, provider, model).

### Existing Infrastructure (Frontend)
- `frontend/src/types/amux-bridge.d.ts` — AmuxBridge interface (L208-321). Already typed — PRUX-05 replaces unsafe casts, not the type definition.
- `frontend/src/components/StatusBar.tsx` — Electron status bar with daemon health, approvals, events. Source of status patterns.
- `frontend/src/components/SetupOnboardingPanel.tsx` — Existing setup prereq checker with localStorage state.
- `frontend/src/lib/agentStore.ts` — AgentSettings store with provider configs.
- `frontend/src/lib/settingsStore.ts` — Settings persistence via readPersistedJson/scheduleJsonWrite.

### Existing Infrastructure (TUI)
- `crates/amux-tui/src/state/concierge.rs` — TUI concierge state with welcome_visible, welcome_actions, loading.
- `crates/amux-tui/src/widgets/concierge.rs` — TUI concierge rendering with action buttons and hit testing.
- `crates/amux-tui/src/widgets/sidebar.rs` — TUI sidebar with thread/task indicators.

### Phase 9 Handoff
- `crates/amux-cli/src/setup_wizard.rs` — Setup wizard (Phase 9). First-run detection, provider config. Concierge onboarding picks up where this leaves off. D-06 adds tier self-assessment question to this wizard.

### Protocol
- `crates/amux-protocol/src/messages.rs` — ClientMessage/DaemonMessage enums, ConciergeWelcome event. New tier-related messages will be added here.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `operator_model.rs` — Full behavioral tracking system. Tier assignment can be derived from existing `CognitiveStyle`, `RiskFingerprint`, `SessionRhythm` signals without new data collection.
- `concierge.rs` — Engine with LLM-powered welcome generation, action buttons, detail levels. Onboarding extends this with tier-adapted prompts and goal run guidance.
- `amux-bridge.d.ts` — `AmuxBridge` interface fully typed. `getBridge()` is a thin wrapper, not a new abstraction.
- `StatusBar.tsx` — Daemon health check pattern (10s polling), event counters. Extend for unified status model.
- `SetupOnboardingPanel.tsx` — localStorage state pattern for onboarding dismissal. Reuse for concierge skip tracking.
- `setup_wizard.rs` — First-run detection (`needs_setup()`). Add tier self-assessment question here.

### Established Patterns
- Zustand stores with `hydrate*()` functions for state persistence (all frontend stores follow this).
- IPC bridge pattern: `window.amux?.methodName()` with optional chaining.
- TUI state follows reducer pattern (`ConciergeState`, `ChatState`, etc.).
- Protocol messages use tagged enums with `#[serde(rename_all = "snake_case")]`.

### Integration Points
- `setup_wizard.rs` — Add tier self-assessment question (D-06), pass result to daemon config.
- `operator_model.rs` — Add `CapabilityTier` enum and `current_tier()` method based on behavioral signals.
- `concierge.rs` — Add onboarding flow logic with tier-adapted content (D-07, D-08).
- `messages.rs` — Add tier-related protocol messages (TierChanged, FeatureDisclosure).
- All frontend components — Replace `(window as any)` casts with `getBridge()` import.
- `StatusBar.tsx` / `sidebar.rs` / CLI output — Implement unified status model (D-11).

</code_context>

<specifics>
## Specific Ideas

- good_ux.md emphasizes the "autonomy ↔ control" trade-off as the key design challenge. Tiers solve this: Newcomers get simple auto modes, Experts get full control.
- "First 15 minutes" (TTFV) is the critical moment — the guided first goal run IS the product demo.
- Collapsed sections (not invisible) for hidden features ensures power users who arrive via the setup wizard's self-assessment don't miss features they'd want.
- One-feature-per-session disclosure prevents the "wall of features" effect that good_ux.md warns about.
- Concierge messages feel like a colleague ("You've been using X — did you know Y?"), not a product tour.

</specifics>

<deferred>
## Deferred Ideas

- Per-task "safety mode" (advise/plan/execute) — mentioned in good_ux.md as gap #4, could be a future phase
- Scope policies (file/network/CLI) + dry-run + rollback UX — good_ux.md gap #5, significant scope
- "Time-travel" UI for error recovery — good_ux.md mentions this as ideal for R users
- Enterprise observability (GenAI semconv) — good_ux.md gap #9, important for D persona
- Agent regression test package + dashboards — good_ux.md gap #10
- Template library + "language → parameters" conversion for goal runs — good_ux.md capability matrix
- Privacy UI for memory/retention management — good_ux.md privacy row
- Token budget / cost-limiting telemetry — good_ux.md performance row

None of these are scope creep — they're valid future work identified by the UX analysis.

</deferred>

---

*Phase: 10-progressive-ux*
*Context gathered: 2026-03-24*
