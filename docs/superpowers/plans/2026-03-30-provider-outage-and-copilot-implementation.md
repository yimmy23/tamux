# Provider Outage Recovery And GitHub Copilot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make provider outages actionable and honest across daemon, TUI, and React, and add GitHub Copilot as a validated provider with both device-flow and token/env auth so it can be used as a real fallback.

**Architecture:** The daemon becomes the source of truth for provider outage diagnostics, eligibility-filtered alternatives, and GitHub Copilot auth/model validity. TUI and React reuse existing config update flows and provider-auth screens to render persistent outage banners and one-click provider+model switching without duplicating eligibility logic.

**Tech Stack:** Rust workspace (`amux-daemon`, `amux-tui`, `amux-protocol`), React/TypeScript frontend, existing daemon bridge/provider auth flows, GitHub official Copilot auth/model docs.

---

## File Map

### Daemon

- Modify: `crates/amux-daemon/src/agent/engine.rs`
  - Add eligibility-filtered provider alternative selection.
  - Add structured outage payload builders.
- Modify: `crates/amux-daemon/src/agent/types.rs`
  - Extend provider outage event and any shared DTOs.
- Modify: `crates/amux-daemon/src/agent/agent_loop.rs`
  - Replace misleading circuit-breaker fallback text with eligibility-filtered messaging.
- Modify: `crates/amux-daemon/src/agent/config.rs`
  - Teach provider auth state reporting about GitHub Copilot auth validity.
- Modify: `crates/amux-daemon/src/agent/capability_tier.rs`
  - Extend status snapshot/provider health payload with outage details and alternatives.
- Modify: `crates/amux-daemon/src/agent/concierge.rs`
  - Emit richer provider outage metadata from concierge circuit-breaker paths.
- Modify: `crates/amux-daemon/src/server.rs`
  - Wire enriched provider outage events and any new provider auth commands.
- Modify: `crates/amux-daemon/src/agent/provider_resolution.rs`
  - Reuse/codify effective model/base URL validation for eligibility checks.
- Modify: `crates/amux-daemon/src/agent/llm_client.rs`
  - Add GitHub Copilot validation/model-fetch integration if validation is handled here.
- Create or modify: `crates/amux-daemon/src/agent/copilot_auth.rs`
  - Device-flow state, token/env auth normalization, validation, model discovery.

### Protocol

- Modify: `crates/amux-protocol/src/messages.rs`
  - Extend status response and any provider outage/provider auth wire messages.

### TUI

- Modify: `crates/amux-tui/src/client.rs`
  - Parse richer provider outage and status payloads.
- Modify: `crates/amux-tui/src/providers.rs`
  - Register GitHub Copilot as a selectable provider in TUI flows.
- Modify: `crates/amux-tui/src/app/events.rs`
  - Route outage events into app state and banner actions.
- Modify: `crates/amux-tui/src/state/config.rs`
  - Add atomic provider+model switch action if missing.
- Modify: `crates/amux-tui/src/app/settings_handlers.rs`
  - Reuse provider/model switch flow from outage actions.
- Modify: `crates/amux-tui/src/widgets/chat.rs`
  - Render inline outage actions in chat/error surfaces.
- Modify or create: `crates/amux-tui/src/widgets/status.rs`
  - Persistent degraded-provider banner/panel.

### React

- Modify: `frontend/src/lib/bridge.ts`
  - Add any new daemon bridge calls needed for atomic provider switching or Copilot auth.
- Modify: `frontend/src/lib/statusStore.ts`
  - Store richer outage payload from daemon status snapshot.
- Modify: `frontend/src/components/StatusBar.tsx`
  - Make degraded-provider status actionable and open banner/panel.
- Create: `frontend/src/components/provider-health/ProviderOutageBanner.tsx`
  - Persistent outage banner and switch/auth actions.
- Modify: `frontend/src/components/agent-chat-panel/runtime.tsx`
  - Reuse existing config update path for provider/model switch actions.
- Modify: `frontend/src/lib/agentStore.ts`
  - Add GitHub Copilot provider definition/auth state wiring.
- Modify: `frontend/src/components/settings-panel/ProviderAuthTab.tsx`
  - Add GitHub Copilot device-flow and token/env auth UX.
- Modify: `frontend/src/types/amux-bridge.d.ts`
  - Add any Copilot auth bridge methods and richer status payload typing.

### Tests

- Modify: daemon tests near `engine.rs`, `config.rs`, `capability_tier.rs`, `server.rs`
- Modify: TUI tests near `state/config.rs`, `app/events.rs`, `widgets/chat.rs`
- Modify: frontend tests or smoke-check scaffolding if present; otherwise capture manual verification steps

---

### Task 1: Fix Provider Alternative Eligibility In The Daemon

**Files:**
- Modify: `crates/amux-daemon/src/agent/engine.rs`
- Modify: `crates/amux-daemon/src/agent/provider_resolution.rs`
- Test: `crates/amux-daemon/src/agent/engine.rs`

- [ ] **Step 1: Write the failing daemon tests for alternative filtering**

Add tests covering:
- placeholder provider row with empty api key/model/base URL is excluded
- failed provider is excluded
- provider with open breaker is excluded
- configured healthy provider is included

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon provider_alternative -- --nocapture`
Expected: FAIL because current logic only checks breaker state.

- [ ] **Step 3: Implement eligibility-filtered alternative selection**

Implement a helper in `engine.rs` that:
- resolves effective provider config
- checks required credentials by auth mode
- rejects empty model/base URL
- rejects failed/open providers

Keep the helper daemon-owned so UIs do not replicate the logic.

- [ ] **Step 4: Run targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon provider_alternative -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/engine.rs crates/amux-daemon/src/agent/provider_resolution.rs
git commit -m "fix: filter provider fallback suggestions by eligibility"
```

### Task 2: Extend Provider Outage Events And Status Snapshot

**Files:**
- Modify: `crates/amux-daemon/src/agent/types.rs`
- Modify: `crates/amux-daemon/src/agent/engine.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop.rs`
- Modify: `crates/amux-daemon/src/agent/concierge.rs`
- Modify: `crates/amux-daemon/src/agent/capability_tier.rs`
- Modify: `crates/amux-protocol/src/messages.rs`
- Modify: `crates/amux-daemon/src/server.rs`
- Test: `crates/amux-daemon/src/agent/types.rs`
- Test: `crates/amux-daemon/src/agent/capability_tier.rs`

- [ ] **Step 1: Write failing tests for richer outage payloads**

Add tests that assert:
- outage event includes failed provider, model, trip count, reason, alternatives
- status snapshot includes degraded-provider details, not just `can_execute` and `trip_count`

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon provider_circuit -- --nocapture`
Expected: FAIL because current payload is too small.

- [ ] **Step 3: Extend the wire/event structures**

Update:
- `AgentEvent::ProviderCircuitOpen`
- status snapshot/provider health JSON
- protocol messages if needed for typed clients

Prefer additive changes where possible so rollout stays compatible.

- [ ] **Step 4: Wire the new payload in server broadcast/status code**

Ensure the richer event survives daemon-to-client forwarding and appears in status polling responses.

- [ ] **Step 4a: Update all provider outage emit sites and fallback text**

Update the main agent loop and concierge circuit-breaker emitters so every `ProviderCircuitOpen` path produces the richer payload, and replace the old misleading human-readable fallback string with eligibility-filtered messaging.

- [ ] **Step 5: Run targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon provider_circuit -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/types.rs crates/amux-daemon/src/agent/engine.rs crates/amux-daemon/src/agent/agent_loop.rs crates/amux-daemon/src/agent/concierge.rs crates/amux-daemon/src/agent/capability_tier.rs crates/amux-protocol/src/messages.rs crates/amux-daemon/src/server.rs
git commit -m "feat: expose actionable provider outage metadata"
```

### Task 3: Add Atomic Provider+Model Switch Support

**Files:**
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-daemon/src/agent/config.rs`
- Modify: `crates/amux-protocol/src/messages.rs`
- Modify: `frontend/src/types/amux-bridge.d.ts`
- Modify: `frontend/src/lib/bridge.ts`
- Modify: `frontend/src/components/agent-chat-panel/runtime.tsx`
- Modify: `crates/amux-tui/src/client.rs`
- Modify: `crates/amux-tui/src/state/config.rs`
- Test: `crates/amux-daemon/src/agent/config.rs`

- [ ] **Step 1: Write failing tests for atomic provider+model switching**

Add tests ensuring a switch action:
- updates provider and model together
- preserves canonical base URL for non-custom providers
- leaves old config unchanged on validation failure

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon provider_switch -- --nocapture`
Expected: FAIL because no dedicated atomic path exists yet.

- [ ] **Step 3: Implement daemon-side atomic switch command/helper**

Reuse existing config persistence instead of inventing a parallel runtime-only path.

- [ ] **Step 4: Add protocol and bridge support for atomic switching**

Expose the atomic provider+model switch through an explicit wire command and client bindings so both TUI and React can use the same daemon-owned path.

- [ ] **Step 5: Wire existing React/TUI config update flows to call the atomic path**

Do not duplicate provider canonicalization in both clients; clients should trigger the daemon-owned switch behavior.

- [ ] **Step 6: Run targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon provider_switch -- --nocapture`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/config.rs crates/amux-protocol/src/messages.rs frontend/src/types/amux-bridge.d.ts frontend/src/lib/bridge.ts frontend/src/components/agent-chat-panel/runtime.tsx crates/amux-tui/src/client.rs crates/amux-tui/src/state/config.rs
git commit -m "feat: add atomic provider model switching"
```

### Task 4: Add Persistent React Outage Banner And Inline Actions

**Files:**
- Modify: `frontend/src/lib/statusStore.ts`
- Modify: `frontend/src/components/StatusBar.tsx`
- Create: `frontend/src/components/provider-health/ProviderOutageBanner.tsx`
- Modify: `frontend/src/components/agent-chat-panel/runtime.tsx`
- Modify: `frontend/src/types/amux-bridge.d.ts`

- [ ] **Step 1: Write the failing UI state test or smoke scaffold**

At minimum, add a focused state-level test or deterministic harness proving:
- degraded provider snapshot creates banner state
- inline action payload maps to provider+model switch intent

If no formal frontend test harness exists, document a local smoke component/test scaffold in the plan implementation.

- [ ] **Step 2: Run the targeted frontend check to verify the current UX is missing**

Run: `cd frontend && npm run build`
Expected: build passes, but no outage banner exists yet; capture this as the baseline before implementation.

- [ ] **Step 3: Implement persistent outage banner UI**

The banner must show:
- failed provider/model
- concise reason
- daemon-approved switch actions
- settings/auth fallback when no alternatives exist

- [ ] **Step 4: Add inline outage actions where failures are rendered**

Reuse the same action wiring as the persistent banner to avoid drift.

- [ ] **Step 5: Run frontend verification**

Run:
- `cd frontend && npm run build`
- `cd frontend && npm run lint`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/statusStore.ts frontend/src/components/StatusBar.tsx frontend/src/components/provider-health/ProviderOutageBanner.tsx frontend/src/components/agent-chat-panel/runtime.tsx frontend/src/types/amux-bridge.d.ts
git commit -m "feat: add react provider outage banner and actions"
```

### Task 5: Add Persistent TUI Outage Surface And Switch Actions

**Files:**
- Modify: `crates/amux-tui/src/client.rs`
- Modify: `crates/amux-tui/src/app/events.rs`
- Modify: `crates/amux-tui/src/widgets/chat.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers.rs`
- Modify or create: `crates/amux-tui/src/widgets/status.rs`
- Test: `crates/amux-tui/src/app/events.rs`

- [ ] **Step 1: Write the failing TUI projection/state tests**

Cover:
- outage event reaches app state
- persistent banner/status surface appears
- switch action triggers provider+model change path

- [ ] **Step 2: Run targeted TUI tests to verify they fail**

Run: `cargo test -p tamux-tui provider_outage -- --nocapture`
Expected: FAIL because the event is not projected into durable UI yet.

- [ ] **Step 3: Implement TUI outage state + banner**

Prefer a compact persistent surface plus a focused action list rather than transient status text only.

- [ ] **Step 4: Add inline outage actions in TUI chat/error surfaces**

Use the daemon-provided alternatives directly.

- [ ] **Step 5: Run targeted TUI tests to verify they pass**

Run: `cargo test -p tamux-tui provider_outage -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/amux-tui/src/client.rs crates/amux-tui/src/app/events.rs crates/amux-tui/src/widgets/chat.rs crates/amux-tui/src/app/settings_handlers.rs crates/amux-tui/src/widgets/status.rs
git commit -m "feat: add tui provider outage banner and actions"
```

### Task 6: Add GitHub Copilot Provider Definition And Auth Plumbing

**Files:**
- Modify: `crates/amux-daemon/src/agent/types.rs`
- Modify: `crates/amux-daemon/src/agent/config.rs`
- Modify: `crates/amux-daemon/src/server.rs`
- Create or modify: `crates/amux-daemon/src/agent/copilot_auth.rs`
- Modify: `crates/amux-tui/src/providers.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers.rs`
- Modify: `crates/amux-tui/src/client.rs`
- Modify: `frontend/src/lib/agentStore.ts`
- Modify: `frontend/src/components/settings-panel/ProviderAuthTab.tsx`
- Modify: `frontend/src/types/amux-bridge.d.ts`
- Test: `crates/amux-daemon/src/agent/config.rs`

- [ ] **Step 1: Write failing tests for Copilot provider auth state**

Cover:
- Copilot provider appears in provider auth states
- device-flow auth valid -> authenticated
- token/env auth valid -> authenticated
- no auth -> not authenticated

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon copilot_auth -- --nocapture`
Expected: FAIL because Copilot provider/auth plumbing does not exist yet.

- [ ] **Step 3: Implement Copilot provider definition and persistence**

Add:
- provider definition
- auth mode handling
- storage for device-flow state
- token/env normalization

- [ ] **Step 4: Expose Copilot through daemon bridge and frontend auth UI**

Reuse existing provider-auth/state machinery instead of creating a separate panel.

- [ ] **Step 4a: Expose Copilot through TUI provider/auth flows**

Add Copilot to the TUI provider registry and auth/settings handling so it can be authenticated and selected there too.

- [ ] **Step 5: Run targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon copilot_auth -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/types.rs crates/amux-daemon/src/agent/config.rs crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/copilot_auth.rs crates/amux-tui/src/providers.rs crates/amux-tui/src/app/settings_handlers.rs crates/amux-tui/src/client.rs frontend/src/lib/agentStore.ts frontend/src/components/settings-panel/ProviderAuthTab.tsx frontend/src/types/amux-bridge.d.ts
git commit -m "feat: add github copilot provider auth support"
```

### Task 7: Add GitHub Copilot Validation And Model Discovery

**Files:**
- Modify: `crates/amux-daemon/src/agent/copilot_auth.rs`
- Modify: `crates/amux-daemon/src/agent/llm_client.rs`
- Modify: `crates/amux-daemon/src/agent/config.rs`
- Modify: `crates/amux-tui/src/providers.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers.rs`
- Modify: `frontend/src/components/settings-panel/ProviderAuthTab.tsx`
- Test: `crates/amux-daemon/src/agent/llm_client.rs`

- [ ] **Step 1: Write failing tests for Copilot validation/model eligibility**

Cover:
- authenticated Copilot with resolved models is eligible
- authenticated Copilot with no resolved models is not eligible
- outage fallback does not suggest Copilot until validation succeeds

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon copilot_validation -- --nocapture`
Expected: FAIL because model discovery/eligibility is not implemented.

- [ ] **Step 3: Implement validation/model discovery**

Use the official GitHub Copilot constraints from the approved spec:
- device-flow auth path
- token/env auth path
- model availability determined by validation/entitlement, not a blind static list

If a safe static fallback list is needed for UI placeholders, do not use it for eligibility decisions.

- [ ] **Step 4: Feed Copilot into outage eligibility helper**

Only include Copilot when validation says it is genuinely usable.

- [ ] **Step 4a: Wire validated Copilot models into TUI selection flows**

Ensure the TUI uses the validated Copilot model set and does not expose stale or placeholder model choices.

- [ ] **Step 5: Run targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon copilot_validation -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/copilot_auth.rs crates/amux-daemon/src/agent/llm_client.rs crates/amux-daemon/src/agent/config.rs crates/amux-tui/src/providers.rs crates/amux-tui/src/app/settings_handlers.rs frontend/src/components/settings-panel/ProviderAuthTab.tsx
git commit -m "feat: validate github copilot models for provider fallback"
```

### Task 8: End-To-End Verification And Cleanup

**Files:**
- Modify: docs or comments only if implementation changed interfaces materially

- [ ] **Step 1: Run daemon test suite for touched areas**

Run: `cargo test -p tamux-daemon`
Expected: PASS

- [ ] **Step 2: Run TUI test suite**

Run: `cargo test -p tamux-tui`
Expected: PASS

- [ ] **Step 3: Run workspace compile verification**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 4: Run frontend verification**

Run:
- `cd frontend && npm run lint`
- `cd frontend && npm run build`

Expected: PASS

- [ ] **Step 5: Manual outage verification**

Verify all of:
- MiniMax breaker open shows persistent banner in React
- MiniMax breaker open shows persistent outage surface in TUI
- only configured/authenticated alternatives appear
- placeholder Hugging Face does not appear
- one-click switch updates provider and model together
- next turn uses the switched provider
- Copilot appears after device-flow login
- Copilot appears after token/env auth when valid
- Copilot does not appear when auth/model validation fails

- [ ] **Step 6: Commit final integration pass**

```bash
git add -A
git commit -m "feat: add actionable provider outage recovery and copilot fallback"
```

## Notes For Implementers

- Follow TDD strictly for daemon and TUI behavior changes.
- Do not compute fallback eligibility in React or TUI; consume daemon truth.
- Prefer additive protocol changes over destructive wire changes.
- Reuse existing provider auth and config update paths wherever possible.
- Keep GitHub Copilot auth state separate from generic raw API-key storage when using device-flow auth.
