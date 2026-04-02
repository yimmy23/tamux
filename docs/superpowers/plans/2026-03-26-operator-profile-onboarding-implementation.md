# Operator Profile Onboarding + Passive Learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add daemon-first operator profiling with first-session concierge onboarding (TUI/React), SQLite-backed profile state, explicit consent gates, weekly/contextual follow-up questions, and `USER.md` sync from DB.

**Architecture:** Extend the daemon protocol and persistence layers first, then implement a bounded operator-profile domain in the daemon, and finally wire TUI/React adapters through existing agent bridge IPC. SQLite is the source of truth; `USER.md` becomes deterministic derived output. Existing concierge onboarding remains entrypoint and is sequenced with profile onboarding.

**Tech Stack:** Rust (amux-daemon/amux-protocol/amux-cli/amux-tui), SQLite via `rusqlite`/`tokio-rusqlite`, Electron IPC (`main.cjs`/`preload.cjs`), React + TypeScript + Zustand.

---

## Scope Check

This spec spans daemon, protocol, and two clients, but these are **not independent subsystems**: they must ship together to expose one coherent onboarding/check-in flow. Keep one plan, with strict task boundaries and integration checkpoints.

## File Structure (planned)

### Protocol + bridge contracts

- Modify: `crates/amux-protocol/src/messages.rs`
- Modify: `crates/amux-cli/src/client.rs` (agent bridge command map + emitted event mapping)

### Daemon persistence + domain

- Modify: `crates/amux-daemon/src/history.rs` (schema + storage helpers)
- Modify: `crates/amux-daemon/src/agent/mod.rs` (module registration)
- Modify: `crates/amux-daemon/src/agent/engine.rs` (engine state wiring)
- Modify: `crates/amux-daemon/src/agent/concierge.rs` (existing onboarding integration)
- Modify: `crates/amux-daemon/src/server.rs` (request path orchestration)
- Modify: `crates/amux-daemon/src/agent/memory.rs` (USER.md arbitration hook)
- Create: `crates/amux-daemon/src/agent/operator_profile/mod.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/model.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/store.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/interview.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/checkins.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/user_sync.rs`

### Electron + React

- Modify: `frontend/electron/main.cjs` (IPC handlers + bridge pending response routing)
- Modify: `frontend/electron/preload.cjs` (new bridge methods)
- Modify: `frontend/src/types/amux-bridge.d.ts` (new bridge typings)
- Modify: `frontend/src/lib/agentStore.ts` (state/actions for operator profile sessions)
- Modify: `frontend/src/App.tsx` (first-run trigger)
- Modify: `frontend/src/CDUIApp.tsx` (first-run trigger)
- Modify: `frontend/src/components/settings-panel/AboutTab.tsx` (profile/consent UI)
- Create: `frontend/src/components/OperatorProfileOnboardingPanel.tsx`

### TUI

- Modify: `crates/amux-tui/src/client.rs` (new protocol client commands/events)
- Modify: `crates/amux-tui/src/app.rs` (first-run/profile question flow, tests)
- Modify: `crates/amux-tui/src/widgets/mod.rs`
- Create: `crates/amux-tui/src/widgets/operator_profile_onboarding.rs`
- (Optional split if `app.rs` grows too large) Create: `crates/amux-tui/src/app/operator_profile.rs`

### Tests/verification targets

- Modify tests in: `crates/amux-protocol/src/messages.rs` (`#[cfg(test)]` section)
- Modify tests in: `crates/amux-daemon/src/agent/concierge.rs`
- Modify tests in: `crates/amux-daemon/src/agent/operator_model.rs` (signal-learning adjacencies if reused)
- Modify tests in: `crates/amux-daemon/src/server.rs` (request orchestration)
- Modify tests in: `crates/amux-tui/src/app.rs`

---

### Task 1: Protocol Contracts for Operator Profile Sessions

**Files:**
- Modify: `crates/amux-protocol/src/messages.rs`
- Test: `crates/amux-protocol/src/messages.rs` (`#[cfg(test)]` roundtrip tests section)

- [ ] **Step 1: Write failing protocol roundtrip tests for new variants**

```rust
#[test]
fn operator_profile_session_messages_roundtrip() {
    let msg = ClientMessage::AgentStartOperatorProfileSession {
        kind: "first_run_onboarding".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentStartOperatorProfileSession { kind } => {
            assert_eq!(kind, "first_run_onboarding");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}
```

- [ ] **Step 2: Run targeted test to verify it fails**

Run: `cargo test -p tamux-protocol operator_profile_session_messages_roundtrip -- --nocapture`  
Expected: FAIL (new variants not defined yet).

- [ ] **Step 3: Add `ClientMessage` / `DaemonMessage` variants and supporting enum/strings**

Add concrete variants from the approved spec:
- start/next/submit/skip/defer/get-summary/set-consent
- session-started/question/progress/summary/session-completed

- [ ] **Step 4: Add serde-safe defaults where needed**

Ensure optional fields use:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
```

- [ ] **Step 5: Re-run protocol tests**

Run: `cargo test -p tamux-protocol messages::tests -- --nocapture`  
Expected: PASS including new operator-profile tests.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-protocol/src/messages.rs
git commit -m "feat(protocol): add operator profile session message contracts"
```

---

### Task 2: Agent Bridge Command Surface (CLI + Electron IPC + Preload Types)

**Files:**
- Modify: `crates/amux-cli/src/client.rs`
- Modify: `frontend/electron/main.cjs`
- Modify: `frontend/electron/preload.cjs`
- Modify: `frontend/src/types/amux-bridge.d.ts`

- [ ] **Step 1: Add failing agent-bridge mapping test or compile assertions in CLI bridge block**

Add a minimal unit-style assertion over command JSON tags (if existing test style allows), or add compile-path TODO test in `client.rs` test module.

- [ ] **Step 2: Run CLI crate tests (expect fail or missing mapping)**

Run: `cargo test -p tamux -- --nocapture`  
Expected: FAIL or missing command coverage for new bridge commands.

- [ ] **Step 3: Extend `AgentBridgeCommand` and match arms**

Add new command JSON tags and map to protocol variants:
- `start-operator-profile-session`
- `next-operator-profile-question`
- `submit-operator-profile-answer`
- `skip-operator-profile-question`
- `defer-operator-profile-question`
- `get-operator-profile-summary`
- `set-operator-profile-consent`

- [ ] **Step 4: Extend Electron `main.cjs` IPC handlers + pending response matcher**

Add `ipcMain.handle(...)` wrappers that use existing `sendAgentCommand` / `sendAgentQuery` conventions and include new response types in pending FIFO resolver list.

- [ ] **Step 5: Expose preload methods + TS types**

Update `bridgeApi` and `AmuxBridge` typings with exact signatures.

- [ ] **Step 6: Re-run bridge-adjacent checks**

Run: `cargo test -p tamux -- --nocapture`  
Run: `cd frontend && npm run build`  
Expected: PASS compile/type checks for bridge API surfaces.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-cli/src/client.rs frontend/electron/main.cjs frontend/electron/preload.cjs frontend/src/types/amux-bridge.d.ts
git commit -m "feat(bridge): expose operator profile session commands across IPC"
```

---

### Task 3: SQLite Schema + Store APIs for Operator Profile

**Files:**
- Modify: `crates/amux-daemon/src/history.rs`

- [ ] **Step 1: Write failing schema/helper tests**

Add tests for:
- table creation (`operator_profile_fields`, `operator_profile_consents`, `operator_profile_events`, `operator_profile_checkins`)
- read/write roundtrip for field value + confidence
- migration safety when table absent

- [ ] **Step 2: Run daemon tests targeting history**

Run: `cargo test -p tamux-daemon history -- --nocapture`  
Expected: FAIL before schema and helper methods exist.

- [ ] **Step 3: Add schema DDL in `init_schema`**

Add `CREATE TABLE IF NOT EXISTS ...` statements and indices following existing patterns.

- [ ] **Step 4: Add typed store helpers**

Implement helper methods for:
- upsert/get profile fields
- upsert/get consents
- append/list profile events
- upsert/list checkins

- [ ] **Step 5: Add transaction-safe migration helper calls where needed**

Use existing `ALTER TABLE` helper patterns (`add_optional_column` style) for forward compatibility.

- [ ] **Step 6: Re-run history tests**

Run: `cargo test -p tamux-daemon history -- --nocapture`  
Expected: PASS for new schema/helper tests.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-daemon/src/history.rs
git commit -m "feat(daemon): add operator profile sqlite schema and store helpers"
```

---

### Task 4: Operator Profile Domain Module (Model + Interview State Machine)

**Files:**
- Create: `crates/amux-daemon/src/agent/operator_profile/model.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/store.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/interview.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/mod.rs`
- Modify: `crates/amux-daemon/src/agent/mod.rs`
- Modify: `crates/amux-daemon/src/agent/engine.rs`

- [ ] **Step 1: Write failing pure unit tests for question selection**

Include tests for:
- first-run asks missing required fields first
- one-question-at-a-time ordering
- skip/defer behavior
- completion threshold logic

- [ ] **Step 2: Run targeted daemon tests**

Run: `cargo test -p tamux-daemon operator_profile -- --nocapture`  
Expected: FAIL before implementation.

- [ ] **Step 3: Implement profile model and interview planner**

Define field keys and confidence-bearing records:

```rust
pub struct ProfileFieldValue {
    pub value_json: String,
    pub confidence: f64,
    pub source: String,
    pub updated_at: u64,
}
```

Implement next-question selector with deterministic ordering.

- [ ] **Step 4: Wire module into `AgentEngine`**

Add bounded runtime state + constructor wiring in `engine.rs`, register module in `agent/mod.rs`.

- [ ] **Step 5: Re-run operator-profile tests**

Run: `cargo test -p tamux-daemon operator_profile -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/operator_profile crates/amux-daemon/src/agent/mod.rs crates/amux-daemon/src/agent/engine.rs
git commit -m "feat(daemon): add operator profile domain and interview planner"
```

---

### Task 5: Concierge Entry-Point Integration in Server Flow

**Files:**
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-daemon/src/agent/concierge.rs`

- [ ] **Step 1: Write failing server/concierge tests for sequencing**

Test cases:
- when `tier.onboarding_completed=false`: tier onboarding runs first, then profile session starts
- when tier complete but profile incomplete: profile question emitted
- when both complete: standard welcome behavior preserved

- [ ] **Step 2: Run targeted tests**

Run: `cargo test -p tamux-daemon concierge -- --nocapture`  
Expected: FAIL before integration changes.

- [ ] **Step 3: Implement orchestration branch**

Update `ClientMessage::AgentRequestConciergeWelcome` handling to:
1. keep existing tier onboarding behavior,
2. invoke operator-profile session start/next question branch,
3. avoid double emission (welcome + question in same tick).

- [ ] **Step 4: Emit profile question/event payloads via existing event channel**

Prefer `DaemonMessage` direct responses for query APIs and `AgentEvent` broadcast for UI stream updates where appropriate.

- [ ] **Step 5: Re-run server + concierge tests**

Run: `cargo test -p tamux-daemon server concierge -- --nocapture`  
Expected: PASS with no regression to existing concierge tests.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/concierge.rs
git commit -m "feat(concierge): sequence tier onboarding with operator profile interview"
```

---

### Task 6: USER.md Arbitration and Legacy Migration

**Files:**
- Modify: `crates/amux-daemon/src/agent/memory.rs`
- Create: `crates/amux-daemon/src/agent/operator_profile/user_sync.rs`
- Modify: `crates/amux-daemon/src/agent/operator_profile/mod.rs`

- [ ] **Step 1: Write failing tests for dual-write prevention**

Test cases:
- direct `MemoryTarget::User` append while sync state is `reconciling` does not produce conflicting final file content
- legacy write is staged into profile events/fields and re-rendered through canonical sync path

- [ ] **Step 2: Run targeted tests**

Run: `cargo test -p tamux-daemon memory -- --nocapture`  
Expected: FAIL before arbitration logic.

- [ ] **Step 3: Implement canonical sync path**

Implement DB -> renderer -> `USER.md` writer, with `clean|dirty|reconciling` state transitions.

- [ ] **Step 4: Implement legacy import bootstrap**

On first profile init, parse/import existing `USER.md` content into `operator_profile_fields` using `source=legacy_import`.

- [ ] **Step 5: Re-run memory/profile sync tests**

Run: `cargo test -p tamux-daemon memory operator_profile -- --nocapture`  
Expected: PASS with deterministic `USER.md` output.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/memory.rs crates/amux-daemon/src/agent/operator_profile
git commit -m "feat(memory): enforce db-first USER.md sync with legacy migration"
```

---

### Task 7: Passive Learning + Weekly/Contextual Check-ins

**Files:**
- Create: `crates/amux-daemon/src/agent/operator_profile/checkins.rs`
- Modify: `crates/amux-daemon/src/agent/operator_profile/mod.rs`
- Modify: `crates/amux-daemon/src/agent/operator_model.rs` (reuse signal hooks)

- [ ] **Step 1: Write failing tests for trigger thresholds**

Cover:
- confidence decay trigger (`< 0.60` and stale >30d)
- behavior delta trigger (`>=20%` divergence)
- missing critical fields trigger (`preferred_name`, `primary_goals`)
- anti-spam limits (72h contextual cooldown, max 2 per active session)
- suppression during critical active goal execution windows

- [ ] **Step 2: Run targeted tests**

Run: `cargo test -p tamux-daemon operator_profile_checkins -- --nocapture`  
Expected: FAIL before trigger engine exists.

- [ ] **Step 3: Implement trigger evaluator + scheduler metadata writes**

Store checkin scheduling/execution in `operator_profile_checkins`.

- [ ] **Step 4: Wire passive signals from existing paths**

Reuse:
- `record_operator_message` stats/revision signals
- approval request/resolution outcomes
- behavioral event stream topic recurrence (MVP: existing events only)

- [ ] **Step 5: Implement consent-gated behavior policy at daemon boundary**

Enforce:
- `passive_learning=false` -> no passive profile confidence updates
- `weekly_checkins=false` -> no weekly scheduling/emission
- `proactive_suggestions=false` -> no proactive suggestion trigger paths

Add unit tests proving behavior is blocked when consent is off.

- [ ] **Step 6: Re-run check-in and operator-model related tests**

Run: `cargo test -p tamux-daemon operator_model operator_profile -- --nocapture`  
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-daemon/src/agent/operator_profile crates/amux-daemon/src/agent/operator_model.rs
git commit -m "feat(daemon): add passive learning and weekly/contextual check-in triggers"
```

---

### Task 7.1: Error-Path and Diagnostics Hardening

**Files:**
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-daemon/src/agent/operator_profile/mod.rs`
- Modify: `crates/amux-daemon/src/agent/operator_profile/store.rs`
- Modify: `crates/amux-daemon/src/agent/operator_profile/user_sync.rs`
- Modify: `crates/amux-tui/src/app.rs`
- Modify: `frontend/src/lib/agentStore.ts`

- [ ] **Step 1: Write failing daemon tests for required failure behavior**

Cover:
- DB read/write failure returns protocol error payloads (not silent fallback)
- scheduler failure falls back to contextual-only mode
- `USER.md` sync failure marks sync-dirty state and preserves DB updates

- [ ] **Step 2: Run targeted daemon tests**

Run: `cargo test -p tamux-daemon operator_profile server -- --nocapture`  
Expected: FAIL before hardening.

- [ ] **Step 3: Implement protocol-visible error surfaces**

Ensure daemon returns explicit error responses/events when onboarding/check-in operations fail.

- [ ] **Step 4: Implement client warning surfacing**

TUI/React should display non-blocking warnings with retry action for operator-profile operations.

- [ ] **Step 5: Add diagnostics visibility for sync-dirty/scheduler fallback**

Expose current state in existing status/diagnostic surfaces so failure mode is inspectable.

- [ ] **Step 6: Re-run daemon + client validations**

Run: `cargo test -p tamux-daemon operator_profile server -- --nocapture`  
Run: `cargo test -p tamux-tui -- --nocapture`  
Run: `cd frontend && npm run build`  
Expected: PASS with hardened error behavior.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/operator_profile crates/amux-tui/src/app.rs frontend/src/lib/agentStore.ts
git commit -m "fix(operator-profile): harden error paths and diagnostics visibility"
```

---

### Task 8: React Onboarding Panel + Settings Transparency

**Files:**
- Create: `frontend/src/components/OperatorProfileOnboardingPanel.tsx`
- Modify: `frontend/src/lib/agentStore.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/CDUIApp.tsx`
- Modify: `frontend/src/components/settings-panel/AboutTab.tsx`

- [ ] **Step 1: Add store-facing contract first**

Implement typed store actions:
- start session
- fetch next question
- submit/skip/defer
- get summary
- set consents

- [ ] **Step 2: Wire first-run request trigger in App/CDUIApp**

After existing concierge welcome request path, trigger operator-profile onboarding session request when appropriate.

- [ ] **Step 3: Build lightweight one-question panel**

Create panel with:
- prompt text
- input control (based on `input_kind`)
- skip/defer controls
- submit
- progress indicator

- [ ] **Step 4: Add “About You” settings section**

Render profile summary + consent toggles + next check-in info.

- [ ] **Step 5: Validate frontend build**

Run: `cd frontend && npm run build`  
Expected: PASS TypeScript + Vite build.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/OperatorProfileOnboardingPanel.tsx frontend/src/lib/agentStore.ts frontend/src/App.tsx frontend/src/CDUIApp.tsx frontend/src/components/settings-panel/AboutTab.tsx
git commit -m "feat(frontend): add operator profile onboarding panel and settings controls"
```

---

### Task 9: TUI Onboarding Question Flow

**Files:**
- Modify: `crates/amux-tui/src/client.rs`
- Modify: `crates/amux-tui/src/app.rs`
- Modify: `crates/amux-tui/src/widgets/mod.rs`
- Create: `crates/amux-tui/src/widgets/operator_profile_onboarding.rs`

- [ ] **Step 1: Add failing TUI behavior tests in `app.rs`**

Add tests for:
- requesting profile session after concierge flow
- rendering one question at a time
- skip/defer updates state

- [ ] **Step 2: Run TUI tests**

Run: `cargo test -p tamux-tui -- --nocapture`  
Expected: FAIL before handling is implemented.

- [ ] **Step 3: Extend client command/event support**

Add client send helpers and daemon message parsing for new operator-profile protocol variants/events.

- [ ] **Step 4: Add widget rendering and app event handling**

Render question card/modal and update `TuiModel` transitions cleanly.

- [ ] **Step 5: Re-run TUI tests**

Run: `cargo test -p tamux-tui -- --nocapture`  
Expected: PASS, including new onboarding flow tests.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-tui/src/client.rs crates/amux-tui/src/app.rs crates/amux-tui/src/widgets/mod.rs crates/amux-tui/src/widgets/operator_profile_onboarding.rs
git commit -m "feat(tui): add operator profile onboarding question flow"
```

---

### Task 10: Final Integration Validation + Docs Touch-up

**Files:**
- Modify (if behavior/help text changed): `README.md` and/or `docs/how-tamux-works.md`
- Modify (if needed): `docs/skills/operating/memory.md` (`USER.md` derivation note)

- [ ] **Step 1: Run full Rust workspace tests**

Run: `cargo test --workspace`  
Expected: PASS.

- [ ] **Step 2: Run frontend production build**

Run: `cd frontend && npm run build`  
Expected: PASS.

- [ ] **Step 3: Manual smoke checks**

- Start daemon + frontend/TUI.
- Verify first-run onboarding appears once.
- Verify weekly/contextual question guardrails.
- Verify consent gate blocks proactive suggestions when off.
- Verify `USER.md` updates from DB and remains stable.

- [ ] **Step 4: Update user-facing docs if behavior changed**

Keep docs minimal and concrete; no speculative roadmap text.

- [ ] **Step 5: Commit validation/docs**

```bash
git add README.md docs/how-tamux-works.md docs/skills/operating/memory.md
git commit -m "docs: document operator profile onboarding and USER.md sync behavior"
```

---

## Cross-Task Guardrails

- Use **@superpowers:test-driven-development** mindset inside each task:
  - write failing test/check first,
  - implement minimal pass,
  - refactor safely.
- Keep newly created files below 500 LOC; split before reaching limit.
- Preserve backward compatibility for existing config/protocol paths.
- No broad catch-and-ignore additions; propagate or surface errors explicitly.
- Prefer additive migrations; do not break existing `~/.tamux` data.

## Suggested Execution Order

1. Task 1 → 2 → 3 → 4 → 6 → 5 → 7 → 7.1 → 9 → 8 → 10

Reasoning:
- Contracts and storage first.
- Domain logic before client rendering.
- USER.md arbitration after profile domain exists.
- TUI/React can proceed in parallel once protocol + daemon behavior stabilizes.

## Checkpoints for Review

- **Checkpoint A (after Task 3):** protocol + DB + domain compile and tests pass.
- **Checkpoint B (after Task 6):** onboarding sequencing + USER.md arbitration verified.
- **Checkpoint C (after Task 7/7.1):** consent gates + error/diagnostic paths verified in daemon.
- **Checkpoint D (after Task 9/8):** both clients show one-question onboarding, warnings, and consent controls.
- **Checkpoint E (Task 10):** full validation + docs finalized.
