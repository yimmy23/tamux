# Phase 10: Progressive UX - Research

**Researched:** 2026-03-24
**Domain:** Progressive disclosure UX, capability tiering, concierge onboarding, cross-client status parity
**Confidence:** HIGH

## Summary

Phase 10 transforms tamux's deep infrastructure into a felt experience by layering progressive disclosure over existing features. The core work is: (1) a capability tier system driven by operator model signals, (2) concierge-powered onboarding with tier-adapted first goal run, (3) feature visibility gating across all three clients (Electron, TUI, CLI), (4) a typed `getBridge()` helper replacing 39 files of unsafe casts, and (5) unified status visibility across all clients.

The existing codebase provides strong foundations: `OperatorModel` already tracks `CognitiveStyle`, `RiskFingerprint`, `SessionRhythm`, `AttentionTopology`, and `ImplicitFeedback` -- these are the exact behavioral signals needed for tier assignment. `ConciergeEngine` already generates LLM-powered welcome messages with action buttons and 4 detail levels. The `AmuxBridge` type definition is already fully typed in `amux-bridge.d.ts`. The work is primarily about connecting these existing systems together with new tier logic, new protocol messages, and UI gating.

The largest risk is complexity in feature gating across three different client architectures (React/Zustand, Ratatui/crossterm, CLI). The Electron frontend uses Zustand stores for state, the TUI uses reducer-pattern state structs, and the CLI uses direct IPC. Each needs tier-aware visibility, but with different mechanisms.

**Primary recommendation:** Build the `CapabilityTier` enum and tier-resolution logic as a pure function in the daemon first, wire it through the protocol, then layer UI gating on each client. Use collapsed sections (not hidden elements) so all features remain discoverable.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Hybrid tier triggering: operator model signals (session count, tool diversity, goal run usage, risk tolerance) drive automatic promotion, PLUS user self-assessment in setup wizard. Both inputs feed the tier assignment.
- **D-02:** Four tiers: Newcomer, Familiar, Power User, Expert. Each maps to a set of visible features.
- **D-03:** Users can override tier both directions in settings -- manually promote or demote. Power users who reinstall shouldn't be stuck in Newcomer. Newcomers overwhelmed by auto-promotion can step back.
- **D-04:** Feature visibility by tier:
  - **Newcomer:** Basic chat, simple tasks, status bar, concierge. Hidden (collapsed): goal runs, task queue, gateway config, memory controls, sub-agents.
  - **Familiar:** + Goal runs, task queue, gateway/automation config. Hidden (collapsed): sub-agent management, memory/learning controls.
  - **Power User:** + Sub-agent management, advanced settings. Hidden (collapsed): memory/learning controls.
  - **Expert:** Everything visible, no collapsed sections.
- **D-05:** Hidden features appear as collapsed sections (not invisible, not grayed-out). Users can expand to peek but won't be overwhelmed. Balances discovery with simplicity.
- **D-06:** Setup wizard asks experience level via natural language: "How familiar are you with AI agents?" -> "Just getting started" / "I've used chatbots" / "I run automations" / "I build agent systems" -> maps to Newcomer/Familiar/Power User/Expert.
- **D-07:** After setup wizard completes, concierge delivers a guided first goal run: greets user, explains what tamux can do at their tier level, then walks through a hands-on example. This is the TTFV moment -- "first success without configuration."
- **D-08:** Onboarding is tier-adapted:
  - **Newcomer:** "Here's what I can do" + simple first task walkthrough
  - **Familiar:** "Here's what's new since chatbots" + goal run demonstration
  - **Power User:** "Here's your workspace" + advanced features overview
  - **Expert:** "Config loaded. Ready." -- minimal, no walkthrough
- **D-09:** Onboarding is always skippable, no follow-up. One-shot offer. If dismissed, never comes back unless user explicitly asks. Maximum respect for user time.
- **D-10:** Single `getBridge(): AmuxBridge | null` function replaces all 39 `(window as any).tamux ?? (window as any).amux` casts. Returns null when not in Electron. Type-safe, single import. Uses existing `AmuxBridge` interface from `amux-bridge.d.ts`.
- **D-11:** Four status categories must be consistent across TUI, Electron, and CLI:
  1. Agent activity state: idle, thinking, executing tool, waiting for approval, running goal
  2. Current task context: active thread/task/goal, current step, progress if applicable
  3. Resource indicators: provider health (circuit breaker), gateway connections, memory usage, token budget
  4. Recent action summary: last 3-5 autonomous actions with one-line explanations (leverages Phase 3 audit feed)
- **D-12:** Tier transitions announced via in-chat concierge message, not notification popups or settings highlights. Natural concierge voice.
- **D-13:** Features disclosed one at a time, spread over days. When promoted to a new tier, don't dump 5 features at once. Introduce one per session over the next few days via concierge.

### Claude's Discretion
- Exact operator model thresholds for tier promotion (how many sessions, what tool diversity score, etc.)
- Concierge welcome message content and tone per tier
- Status widget layout details per client
- Which specific features map to which collapsed sections in each tier
- How `getBridge()` handles edge cases (e.g., bridge available but daemon disconnected)
- Implementation of the "one feature per session" disclosure queue

### Deferred Ideas (OUT OF SCOPE)
- Per-task "safety mode" (advise/plan/execute) -- good_ux.md gap #4
- Scope policies (file/network/CLI) + dry-run + rollback UX -- good_ux.md gap #5
- "Time-travel" UI for error recovery -- good_ux.md R persona ideal
- Enterprise observability (GenAI semconv) -- good_ux.md gap #9
- Agent regression test package + dashboards -- good_ux.md gap #10
- Template library + "language -> parameters" conversion for goal runs
- Privacy UI for memory/retention management
- Token budget / cost-limiting telemetry
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PRUX-01 | Capability tiers driven by operator model: Newcomer -> Familiar -> Power User -> Expert | `OperatorModel` struct has session_count, tool diversity via `ImplicitFeedback.fallback_histogram`, goal run tracking via `AgentEvent::GoalRunUpdate`, risk tolerance via `RiskFingerprint`. New `CapabilityTier` enum + pure `resolve_tier()` function derives tier from these signals. |
| PRUX-02 | Tier transitions announced naturally via concierge | `ConciergeEngine` already has LLM-powered message generation and `AgentEvent::ConciergeWelcome`. New `AgentEvent::TierChanged` event + disclosure queue in daemon trigger concierge messages. |
| PRUX-03 | New users see simplified interface; advanced capabilities revealed as usage grows | Frontend: Zustand `tierStore` gates section visibility. TUI: `TierState` in state module controls widget rendering. CLI: `--tier` flag or config-based filtering. All use collapsed sections per D-05. |
| PRUX-04 | Concierge onboarding: guided first experience with hands-on examples | Extends `setup_wizard.rs` (D-06 self-assessment) + `concierge.rs` (tier-adapted welcome). New onboarding flow after wizard completes sends first goal run via existing `agentStartGoalRun` IPC. |
| PRUX-05 | Typed `getBridge()` helper replaces all 39 unsafe casts | 9 local `getBridge()` duplicates already exist across `agentStore.ts`, `goalRuns.ts`, `agentTodos.ts`, `persistence.ts`, etc. Consolidate into single `frontend/src/lib/bridge.ts` export. 85 occurrences of `(window as any).(tamux|amux)` across 39 files need replacement. |
| PRUX-06 | Consistent status visibility across TUI, Electron, and CLI | `StatusBar.tsx` has daemon health polling. `AgentEvent` enum has `GoalRunUpdate`, `TaskUpdate`, `GatewayStatus`, `HeartbeatDigest`. New `AgentStatusSnapshot` struct aggregates these into 4 categories (D-11). New protocol message `AgentStatusQuery`/`AgentStatusResponse` for on-demand status. |
</phase_requirements>

## Standard Stack

No new external dependencies needed. All work uses the existing stack.

### Core (Already in Project)
| Library | Version | Purpose | Why Used |
|---------|---------|---------|----------|
| Zustand | 5.x | Frontend state for tier visibility, disclosure queue, status | Existing store pattern used throughout |
| Ratatui | 0.29 | TUI widget rendering with tier-based visibility | Existing TUI framework |
| serde + serde_json | 1.x | Serialization of CapabilityTier, tier config, status snapshot | Existing wire format |
| bincode | 1.x | IPC framing for new protocol messages | Existing protocol codec |
| amux-protocol | internal | New message types for tier and status | Existing IPC protocol crate |

### No New Dependencies
This phase is entirely about layering UX logic over existing infrastructure. No new crate or npm package additions needed.

## Architecture Patterns

### Recommended Changes by Layer

```
crates/amux-daemon/src/agent/
├── capability_tier.rs           # NEW: CapabilityTier enum, resolve_tier() pure fn, tier config
├── operator_model.rs            # MODIFY: add tier tracking to OperatorModel, promotion detection
├── concierge.rs                 # MODIFY: onboarding flow, tier-adapted messages, disclosure queue
├── types.rs                     # MODIFY: new AgentEvent variants, new config types
└── engine.rs                    # MODIFY: wire tier resolution into engine lifecycle

crates/amux-protocol/src/
└── messages.rs                  # MODIFY: new ClientMessage/DaemonMessage variants for tier + status

crates/amux-cli/src/
├── setup_wizard.rs              # MODIFY: add D-06 self-assessment question
└── main.rs                      # MODIFY: tier-aware status output, tamux status subcommand

crates/amux-tui/src/
├── state/tier.rs                # NEW: TierState with tier-based visibility flags
└── widgets/                     # MODIFY: conditional rendering based on tier

frontend/src/
├── lib/bridge.ts                # NEW: shared getBridge() function
├── lib/tierStore.ts             # NEW: Zustand store for capability tier + visibility
├── lib/statusStore.ts           # NEW: Zustand store for unified agent status
└── components/                  # MODIFY: wrap tier-gated sections with CollapsibleTierSection
```

### Pattern 1: Pure Tier Resolution

**What:** Tier assignment computed as a pure function from operator model signals + user override.
**When to use:** Any time the daemon needs to determine the current tier.
**Example:**

```rust
// crates/amux-daemon/src/agent/capability_tier.rs

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    Newcomer,
    Familiar,
    PowerUser,
    Expert,
}

pub struct TierSignals {
    pub session_count: u64,
    pub unique_tools_used: usize,
    pub goal_runs_completed: u64,
    pub risk_tolerance: RiskTolerance,
    pub user_self_assessment: Option<CapabilityTier>,
    pub user_override: Option<CapabilityTier>,
}

/// Pure function: resolve tier from signals. Testable without daemon.
pub fn resolve_tier(signals: &TierSignals) -> CapabilityTier {
    // User override always wins
    if let Some(override_tier) = signals.user_override {
        return override_tier;
    }

    // Behavioral auto-promotion
    let behavioral = if signals.goal_runs_completed >= 10
        && signals.unique_tools_used >= 8
        && signals.risk_tolerance == RiskTolerance::Aggressive
    {
        CapabilityTier::Expert
    } else if signals.goal_runs_completed >= 3 && signals.unique_tools_used >= 5 {
        CapabilityTier::PowerUser
    } else if signals.session_count >= 5 && signals.unique_tools_used >= 3 {
        CapabilityTier::Familiar
    } else {
        CapabilityTier::Newcomer
    };

    // Take the higher of behavioral vs self-assessment
    match signals.user_self_assessment {
        Some(self_tier) if self_tier > behavioral => self_tier,
        _ => behavioral,
    }
}
```

### Pattern 2: Collapsed Section Gating (Frontend)

**What:** React component wrapping tier-gated sections with collapsible disclosure.
**When to use:** Any UI section that should be hidden at lower tiers per D-04/D-05.
**Example:**

```typescript
// frontend/src/components/base-components/TierGatedSection.tsx

function TierGatedSection({
    requiredTier,
    label,
    children,
}: {
    requiredTier: CapabilityTier;
    label: string;
    children: React.ReactNode;
}) {
    const currentTier = useTierStore((s) => s.currentTier);
    const tierOrdinal = TIER_ORDER[currentTier];
    const requiredOrdinal = TIER_ORDER[requiredTier];

    if (tierOrdinal >= requiredOrdinal) {
        // Tier met: render normally
        return <>{children}</>;
    }

    // Tier not met: render as collapsed section (D-05)
    return (
        <CollapsibleSection
            label={label}
            defaultCollapsed={true}
            dimmed={true}
        >
            {children}
        </CollapsibleSection>
    );
}
```

### Pattern 3: Unified Status Snapshot

**What:** A struct aggregating the 4 status categories (D-11) into a single queryable model.
**When to use:** Any client needing to display agent status.
**Example:**

```rust
// In types.rs or a new status module

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusSnapshot {
    // Category 1: Agent activity state
    pub activity: AgentActivityState,
    // Category 2: Current task context
    pub active_thread_id: Option<String>,
    pub active_goal_run: Option<GoalRunSummary>,
    pub active_task: Option<TaskSummary>,
    // Category 3: Resource indicators
    pub provider_health: Vec<ProviderHealthStatus>,
    pub gateway_statuses: Vec<GatewayHealthStatus>,
    // Category 4: Recent actions (leverages Phase 3 audit)
    pub recent_actions: Vec<RecentActionSummary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityState {
    Idle,
    Thinking,
    ExecutingTool,
    WaitingForApproval,
    RunningGoal,
}
```

### Pattern 4: Disclosure Queue (One Feature Per Session)

**What:** A persistent queue of features to disclose, draining one item per session.
**When to use:** When a tier promotion introduces multiple new features (D-13).
**Example:**

```rust
// In capability_tier.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisclosureQueue {
    pub pending_features: Vec<FeatureDisclosure>,
    pub disclosed_features: Vec<String>,
    pub last_disclosure_session: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDisclosure {
    pub feature_id: String,
    pub tier: CapabilityTier,
    pub title: String,
    pub description: String,
}
```

### Anti-Patterns to Avoid
- **Invisible features:** Don't hide features completely -- use collapsed sections per D-05. Users must be able to discover features by expanding.
- **Eager tier promotion:** Don't promote users too quickly. The thresholds should be conservative -- better to under-promote and let users self-elevate than to overwhelm.
- **Multiple disclosures per session:** D-13 is explicit: one feature per session, spread over days. Don't batch disclosures.
- **Separate tier logic per client:** The daemon is the source of truth for tier. Clients read the tier, not compute it.
- **Blocking bridge null:** `getBridge()` returning null must not crash -- every call site must handle the null case gracefully (existing pattern in `persistence.ts`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tier persistence | Custom file format | Extend `OperatorModel` JSON file (already persisted at `~/.tamux/operator_model.json`) | Tier is derived from operator model -- store alongside it |
| Feature gate config | Hardcoded tier-feature map | Data-driven `TierFeatureMap` in config, loaded at startup | Makes tier->feature mapping adjustable without code changes |
| Status polling | Custom timer per status category | Single `AgentStatusQuery` IPC message returning full snapshot | Reduces IPC chattiness; one round trip for all status |
| Onboarding state tracking | Custom localStorage keys per step | Extend existing `SetupPanelState` pattern from `SetupOnboardingPanel.tsx` | Already has localStorage persistence and dismiss tracking |

**Key insight:** The operator model, concierge engine, and bridge types already exist. This phase is about connecting them, not building from scratch.

## Common Pitfalls

### Pitfall 1: Operator Model Disabled by Default
**What goes wrong:** `OperatorModelConfig.enabled` defaults to `false`. Without it, there are no behavioral signals for tier assignment.
**Why it happens:** The operator model was designed as opt-in during earlier phases.
**How to avoid:** Phase 10 must either (a) enable operator model by default when tiers are active, or (b) fall back gracefully to self-assessment-only tier when operator model is disabled. Option (b) is safer for backward compatibility.
**Warning signs:** Tier always stays at the self-assessment level regardless of usage.

### Pitfall 2: Protocol Backward Compatibility
**What goes wrong:** Adding new `ClientMessage`/`DaemonMessage` variants breaks older clients connected to a newer daemon (or vice versa).
**Why it happens:** bincode deserialization fails on unknown enum variants.
**How to avoid:** New protocol messages must use `#[serde(default)]` fields where possible and new variants should be appended to the end of the enum. Test with older client binary connecting to new daemon.
**Warning signs:** "failed to deserialize" errors in IPC logs after upgrade.

### Pitfall 3: getBridge() Return Type Inconsistency
**What goes wrong:** Existing `getBridge()` duplicates return `null` in some files, `undefined` in others (`persistence.ts`). Unifying to `null` breaks code expecting `undefined`.
**Why it happens:** Different authors wrote each duplicate independently.
**How to avoid:** The shared `getBridge()` MUST return `AmuxBridge | null` (matching the majority). Update `persistence.ts` to handle `null` instead of `undefined`. Search for all `getBridge()` call sites and verify null-handling.
**Warning signs:** TypeScript compile errors or runtime `cannot read property of undefined` after migration.

### Pitfall 4: TUI Widget Rendering Performance
**What goes wrong:** Adding tier-based conditional rendering to every TUI widget slows the 50ms tick loop.
**Why it happens:** Checking tier visibility on every render frame adds overhead.
**How to avoid:** Compute tier visibility flags once when tier changes (not per-frame). Store as flat booleans in `TierState`: `show_goal_runs: bool`, `show_subagents: bool`, etc. Widgets check the boolean, not recompute tier logic.
**Warning signs:** TUI frame rate drops or input lag.

### Pitfall 5: Concierge LLM Failure During Onboarding
**What goes wrong:** If the LLM call for onboarding fails (provider down, rate limited, wrong API key), the first-time user sees nothing.
**Why it happens:** Onboarding relies on `ConciergeEngine.compose_welcome()` which calls the LLM.
**How to avoid:** Always have a static template fallback for onboarding, same as the existing `template_fallback()` in `concierge.rs`. The onboarding flow must work without any LLM -- the LLM just makes it better.
**Warning signs:** Empty or missing welcome message on first launch.

### Pitfall 6: Setup Wizard Reads Stdin -- Can't Work in Electron
**What goes wrong:** D-06 adds a self-assessment question to `setup_wizard.rs`, which reads stdin. But Electron launches the daemon without an attached terminal.
**Why it happens:** The setup wizard was designed for CLI first-run.
**How to avoid:** For Electron: self-assessment happens in the frontend (React component) and is sent to daemon via IPC. For CLI/TUI: self-assessment happens in the existing wizard flow. Two paths, same result: a tier stored in config.
**Warning signs:** Electron launch hangs waiting for stdin input.

### Pitfall 7: Circular Dependency Between Tier Store and Agent Store
**What goes wrong:** `tierStore` needs agent config (for tier override), and `agentStore` needs tier (for feature gating). Cross-store subscriptions create update loops.
**Why it happens:** Zustand stores are independent -- cross-references need careful management.
**How to avoid:** Tier is derived from daemon state (not frontend state). `tierStore` subscribes to daemon events only. `agentStore` reads `tierStore` but does not write to it. One-directional data flow: daemon -> tierStore -> components.
**Warning signs:** Infinite re-render loops, React dev tools showing continuous state updates.

## Code Examples

### Shared getBridge() Helper (PRUX-05)

```typescript
// frontend/src/lib/bridge.ts
// Source: based on existing patterns in persistence.ts, agentStore.ts, goalRuns.ts

/**
 * Returns the Electron bridge object, or null when running outside Electron.
 * Single source of truth -- replaces all 39 files of (window as any) casts.
 */
export function getBridge(): AmuxBridge | null {
    if (typeof window === "undefined") return null;
    return window.tamux ?? window.amux ?? null;
}
```

### Tier-Adapted Concierge Onboarding Prompt

```rust
// Extending concierge.rs compose_welcome for onboarding

fn onboarding_system_prompt(tier: CapabilityTier) -> String {
    let tier_context = match tier {
        CapabilityTier::Newcomer => {
            "The user is new to AI agents. Be warm and encouraging. \
             Explain what tamux can do in simple terms. Walk them through \
             sending their first message. Avoid jargon."
        }
        CapabilityTier::Familiar => {
            "The user has used chatbots before. Highlight what makes tamux \
             different: persistent memory, goal runs, background work. \
             Demonstrate a simple goal run."
        }
        CapabilityTier::PowerUser => {
            "The user runs automations. Give a quick overview of the workspace: \
             terminal sessions, task queue, goal runs, sub-agents. Point to \
             settings for customization."
        }
        CapabilityTier::Expert => {
            "The user builds agent systems. Be brief: config loaded, daemon running, \
             all features unlocked. Mention the operator model and skill system."
        }
    };

    format!(
        "You are the tamux concierge. This is the user's first session. \
         {tier_context}\n\n\
         Keep it under 150 words. Be conversational, not robotic. \
         End with one concrete action the user can try right now."
    )
}
```

### Protocol Messages for Tier and Status

```rust
// In messages.rs - new ClientMessage variants

/// Query the agent's current capability tier and status snapshot.
AgentStatusQuery,

/// Set capability tier override (from settings UI).
AgentSetTierOverride {
    tier: Option<String>, // None to clear override, Some("newcomer"|"familiar"|"power_user"|"expert")
},

// In messages.rs - new DaemonMessage variants

/// Agent status snapshot (response to AgentStatusQuery or pushed on change).
AgentStatusResponse {
    tier: String,
    activity: String,
    active_thread_id: Option<String>,
    active_goal_run_id: Option<String>,
    active_goal_run_title: Option<String>,
    provider_health_json: String,
    gateway_statuses_json: String,
    recent_actions_json: String,
},

/// Capability tier changed (pushed to all clients).
AgentTierChanged {
    previous_tier: String,
    new_tier: String,
    reason: String, // "auto_promotion", "user_override", "self_assessment"
},
```

### TUI Tier State

```rust
// crates/amux-tui/src/state/tier.rs

pub struct TierState {
    pub current_tier: String,
    // Pre-computed visibility flags for widget rendering
    pub show_goal_runs: bool,
    pub show_task_queue: bool,
    pub show_gateway_config: bool,
    pub show_memory_controls: bool,
    pub show_subagents: bool,
    pub show_advanced_settings: bool,
}

impl TierState {
    pub fn from_tier(tier: &str) -> Self {
        let tier_ord = match tier {
            "newcomer" => 0,
            "familiar" => 1,
            "power_user" => 2,
            "expert" => 3,
            _ => 0,
        };
        Self {
            current_tier: tier.to_string(),
            show_goal_runs: tier_ord >= 1,
            show_task_queue: tier_ord >= 1,
            show_gateway_config: tier_ord >= 1,
            show_memory_controls: tier_ord >= 3,
            show_subagents: tier_ord >= 2,
            show_advanced_settings: tier_ord >= 2,
        }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hide features completely | Collapsed sections (progressive disclosure) | UX research consensus | Users discover features naturally without being overwhelmed |
| Feature flags as on/off | Behavioral tier-driven visibility | Emerging in agent UX | Features appear when user demonstrates readiness |
| Manual onboarding tours | Concierge-driven conversational onboarding | 2024-2025 | Natural language walkthrough > click-through tutorial |
| Per-platform status widgets | Unified status model served by daemon | tamux Phase 10 design | Single source of truth prevents status drift between clients |

## Open Questions

1. **Tier Promotion Thresholds**
   - What we know: D-01 says hybrid triggering from operator model signals + self-assessment. Signals available: `session_count`, `unique_tools_used` (derivable from `ImplicitFeedback.fallback_histogram`), goal run count (from history), `risk_tolerance`.
   - What's unclear: Exact numerical thresholds. The thresholds in the code example above (5 sessions for Familiar, 3 goal runs for Power User, etc.) are reasonable starting points but untested.
   - Recommendation: Claude's discretion per CONTEXT.md. Use conservative thresholds. Start higher and lower over time based on feedback.

2. **Unique Tools Used Tracking**
   - What we know: `ImplicitFeedback.fallback_histogram` tracks tool fallbacks, but not total unique tools used.
   - What's unclear: Whether a separate `tools_used_set` field is needed in OperatorModel, or if we can derive it from `action_audit` table.
   - Recommendation: Add a `unique_tools_seen: HashSet<String>` (serialized as `Vec<String>`) to `OperatorModel`. Simple to maintain, avoids expensive audit table queries.

3. **Goal Run Count Source**
   - What we know: Goal runs are in the `goal_runs` table in SQLite (via `HistoryStore`). `OperatorModel` does not currently track goal run counts.
   - What's unclear: Whether to count all goal runs or only successful ones for tier promotion.
   - Recommendation: Count all completed (not cancelled) goal runs. Success rate matters less than engagement for tier purposes.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (daemon/protocol), no frontend test framework |
| Config file | None -- inline `#[cfg(test)]` modules |
| Quick run command | `cargo test --lib -p amux-daemon -- capability_tier` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PRUX-01 | resolve_tier() returns correct tier for signal combinations | unit | `cargo test --lib -p amux-daemon -- capability_tier::tests` | Wave 0 |
| PRUX-01 | User override takes precedence over behavioral tier | unit | `cargo test --lib -p amux-daemon -- capability_tier::tests::override` | Wave 0 |
| PRUX-01 | Self-assessment elevates (never demotes) behavioral tier | unit | `cargo test --lib -p amux-daemon -- capability_tier::tests::self_assessment` | Wave 0 |
| PRUX-02 | TierChanged event emitted on promotion | unit | `cargo test --lib -p amux-daemon -- capability_tier::tests::promotion_event` | Wave 0 |
| PRUX-02 | Disclosure queue drains one item per session | unit | `cargo test --lib -p amux-daemon -- capability_tier::tests::disclosure_queue` | Wave 0 |
| PRUX-03 | TierState visibility flags correct for each tier | unit | `cargo test --lib -p amux-tui -- state::tier::tests` | Wave 0 |
| PRUX-04 | Onboarding system prompt adapts to tier | unit | `cargo test --lib -p amux-daemon -- concierge::tests::onboarding_prompt` | Wave 0 |
| PRUX-05 | getBridge() returns null outside Electron | manual-only | TypeScript type check: `npm run build` in frontend | N/A |
| PRUX-05 | No remaining `(window as any)` casts | smoke | `grep -r "(window as any)" frontend/src/ \| wc -l` returns 0 | N/A |
| PRUX-06 | AgentStatusSnapshot serialization round-trip | unit | `cargo test --lib -p amux-daemon -- tests::status_snapshot_serde` | Wave 0 |
| PRUX-06 | Protocol StatusQuery/StatusResponse round-trip | unit | `cargo test --lib -p amux-protocol -- tests::status_messages` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib -p amux-daemon -- capability_tier`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + `grep "(window as any)" frontend/src/` returns 0 + `npm run build` succeeds

### Wave 0 Gaps
- [ ] `crates/amux-daemon/src/agent/capability_tier.rs` -- new module with `#[cfg(test)]` section
- [ ] TUI tier state tests in `crates/amux-tui/src/state/tier.rs`
- [ ] Protocol message tests for new variants in `crates/amux-protocol/src/messages.rs`

## Project Constraints (from CLAUDE.md)

- **Tech stack locked:** Rust daemon + TypeScript/React frontend. No language changes.
- **Local-first:** All tier data, operator model, and disclosure state stored locally in `~/.tamux/`.
- **Backward compatibility:** New protocol messages must not break existing clients. `OperatorModelConfig.enabled` default (`false`) must be respected -- tier works even when operator model is off (falls back to self-assessment only).
- **Platform parity:** Tier gating must work across Linux, macOS, and Windows in all three clients.
- **Rust conventions:** `snake_case` files, `PascalCase` types, `#[derive(Debug, Clone, Serialize, Deserialize)]` for wire types, `#[serde(rename_all = "snake_case")]` for enums.
- **TypeScript conventions:** Named exports only, `function ComponentName()` syntax, Zustand selector pattern `useStore((s) => s.thing)`, strict mode with no unused locals.
- **GSD workflow enforcement:** All changes must go through GSD workflow.

## Sources

### Primary (HIGH confidence)
- `crates/amux-daemon/src/agent/operator_model.rs` -- Full OperatorModel struct with all behavioral signals. Lines 1-360.
- `crates/amux-daemon/src/agent/concierge.rs` -- ConciergeEngine with LLM-powered welcome, 4 detail levels, action buttons. Lines 1-360.
- `crates/amux-daemon/src/agent/types.rs` -- AgentEvent enum (38+ variants), OperatorModelConfig, ConciergeConfig, ConciergeDetailLevel. Lines 1131-2050.
- `crates/amux-protocol/src/messages.rs` -- ClientMessage/DaemonMessage enums with existing concierge messages.
- `frontend/src/types/amux-bridge.d.ts` -- AmuxBridge interface (lines 208-321). Fully typed.
- `frontend/src/components/StatusBar.tsx` -- Existing daemon health check pattern (10s polling), event counters.
- `frontend/src/components/SetupOnboardingPanel.tsx` -- localStorage state pattern for dismissal tracking.
- `crates/amux-cli/src/setup_wizard.rs` -- First-run detection, provider config, stdin-based wizard.
- `crates/amux-tui/src/state/concierge.rs` -- TUI concierge state with reducer pattern.
- `.planning/good_ux.md` -- Comprehensive UX strategy: personas (R/A/D), capability matrix, autonomy/control trade-off, TTFV metrics.

### Secondary (MEDIUM confidence)
- Grep across 39 frontend files confirmed exactly 85 occurrences of `(window as any).(tamux|amux)` pattern.
- 9 local `getBridge()` duplicates found with inconsistent return types (null vs undefined).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, entirely existing infrastructure
- Architecture: HIGH -- patterns follow established project conventions, all integration points verified in source code
- Pitfalls: HIGH -- identified from direct codebase analysis (operator model defaults, bridge inconsistencies, stdin limitation)

**Research date:** 2026-03-24
**Valid until:** 2026-04-24 (stable -- no external dependency changes expected)
