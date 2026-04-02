# Daemon WhatsApp QR Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver one daemon-owned WhatsApp QR linking backend that React, TUI, and `tamux setup` all use through a typed protocol.

**Architecture:** Add a dedicated `whatsapp_link` runtime to the daemon with explicit lifecycle state and subscriber fanout. Extend `amux-protocol` with typed request/response/event variants, then wire server handlers and all clients (CLI bridge + Electron + TUI + setup) to the same contract. Keep WhatsApp Cloud API send path unchanged.

**Tech Stack:** Rust (tokio, serde, bincode), existing `amux-protocol` framed IPC, Electron preload/main IPC, React 19 + TypeScript, existing TUI ratatui state machine.

**Spec:** `docs/superpowers/specs/2026-03-25-daemon-whatsapp-qr-unification-design.md`

---

## Implementation defaults (lock before coding)

- Typed protocol only (no new ad-hoc JSON payload channels).
- Daemon is source of truth for WhatsApp link status and QR lifecycle.
- Subscriber replay semantics: new subscriber gets one immediate `AgentWhatsAppLinkStatus` snapshot, then incremental events in order.
- Setup flow timeout default: 120s for interactive QR linking, then offer skip/retry; Esc cancels cleanly.
- Temporary fallback switch for migration safety:
  - `gateway.whatsapp_link_fallback_electron` (bool, default `false`)
  - Owner: this phase
  - Removal criteria: TUI + setup + React pass all WhatsApp QR UAT checks for one release cycle.

---

## File map

### Modify

- `crates/amux-protocol/src/messages.rs`
- `crates/amux-daemon/src/agent/mod.rs`
- `crates/amux-daemon/src/agent/engine.rs`
- `crates/amux-daemon/src/agent/types.rs`
- `crates/amux-daemon/src/server.rs`
- `crates/amux-cli/src/client.rs`
- `crates/amux-cli/src/setup_wizard.rs`
- `crates/amux-cli/src/main.rs` (only if setup flow return plumbing needs extension)
- `crates/amux-tui/src/state/mod.rs`
- `crates/amux-tui/src/state/modal.rs`
- `crates/amux-tui/src/client.rs`
- `crates/amux-tui/src/app/events.rs`
- `crates/amux-tui/src/app/modal_handlers.rs`
- `crates/amux-tui/src/app/settings_handlers.rs`
- `crates/amux-tui/src/widgets/settings.rs`
- `frontend/electron/main.cjs`
- `frontend/electron/preload.cjs`
- `frontend/src/types/amux-bridge.d.ts`
- `frontend/src/components/settings-panel/GatewayTab.tsx`

### Create

- `crates/amux-daemon/src/agent/whatsapp_link.rs`
- `crates/amux-tui/src/widgets/whatsapp_link.rs` (if modal rendering extracted; preferred for <500 LOC files)

### Tests to add/update

- `crates/amux-protocol/src/messages.rs` (roundtrip tests)
- `crates/amux-daemon/src/agent/whatsapp_link.rs` (`#[cfg(test)]` state-machine and subscriber tests)
- `crates/amux-daemon/src/server.rs` (handler + forwarding tests)
- `crates/amux-cli/src/setup_wizard.rs` (gateway option and post-selection tests)
- `crates/amux-tui/src/widgets/settings.rs` (gateway row/action visibility tests)
- `frontend/electron/main.whatsapp-bridge-config.test.cjs` (migrate to daemon-path assertions)
- `frontend/electron/whatsapp-bridge.test.cjs` (replace/retarget for daemon-owned flow contracts)

---

## Task 1: Add typed protocol surface for WhatsApp link lifecycle

**Files:**
- Modify: `crates/amux-protocol/src/messages.rs`

- [ ] **Step 1: Write failing protocol roundtrip tests for new variants**

Add tests for all new client and daemon variants:

- client: `AgentWhatsAppLinkStart`, `AgentWhatsAppLinkStop`, `AgentWhatsAppLinkStatus`, `AgentWhatsAppLinkSubscribe`, `AgentWhatsAppLinkUnsubscribe`
- daemon: `AgentWhatsAppLinkStatus`, `AgentWhatsAppLinkQr`, `AgentWhatsAppLinked`, `AgentWhatsAppLinkError`, `AgentWhatsAppLinkDisconnected`

- [ ] **Step 2: Run protocol tests and verify failure**

Run:

```bash
cargo test -p tamux-protocol messages
```

Expected: compile/test failure referencing missing variants.

- [ ] **Step 3: Implement protocol variants**

Use explicit fields:

- `AgentWhatsAppLinkStatus { state: String, phone: Option<String>, last_error: Option<String> }`
- `AgentWhatsAppLinkQr { ascii_qr: String, expires_at_ms: Option<u64> }`
- `AgentWhatsAppLinked { phone: Option<String> }`
- `AgentWhatsAppLinkError { message: String, recoverable: bool }`
- `AgentWhatsAppLinkDisconnected { reason: Option<String> }`

- [ ] **Step 4: Re-run protocol tests**

Run:

```bash
cargo test -p tamux-protocol messages
```

Expected: protocol tests pass with bincode + codec roundtrips.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-protocol/src/messages.rs
git commit -m "feat(protocol): add typed whatsapp link ipc messages"
```

---

## Task 2: Implement daemon `whatsapp_link` runtime and state machine

**Files:**
- Create: `crates/amux-daemon/src/agent/whatsapp_link.rs`
- Modify: `crates/amux-daemon/src/agent/mod.rs`
- Modify: `crates/amux-daemon/src/agent/engine.rs`

- [ ] **Step 1: Write failing runtime tests in new module**

Cover:

- start → qr_ready transition emits QR event
- qr refresh event replaces stale QR (no duplicate unchanged payload emission)
- connected transition emits linked event and updates status snapshot
- stop emits disconnected and clears active session
- new subscriber receives immediate latest status snapshot

- [ ] **Step 2: Run daemon tests and verify failure**

Run:

```bash
cargo test -p tamux-daemon whatsapp_link
```

Expected: failing tests due missing runtime implementation.

- [ ] **Step 3: Implement runtime module**

Implement:

- `WhatsAppLinkRuntime` struct (state, subscribers, process handle, retry metadata)
- `WhatsAppLinkState` enum/string mapping (`disconnected|starting|qr_ready|connected|error`)
- methods: `start()`, `stop()`, `status_snapshot()`, `subscribe()`, `unsubscribe()`, `broadcast_*()`
- strict error propagation (no silent fallbacks)

- [ ] **Step 4: Wire runtime into `AgentEngine`**

Add field(s) in `AgentEngine` and initialize in constructor.

- [ ] **Step 5: Re-run daemon runtime tests**

Run:

```bash
cargo test -p tamux-daemon whatsapp_link
```

Expected: new runtime tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-daemon/src/agent/whatsapp_link.rs crates/amux-daemon/src/agent/mod.rs crates/amux-daemon/src/agent/engine.rs
git commit -m "feat(daemon): add whatsapp link runtime state machine"
```

---

## Task 3: Add daemon server handlers for typed WhatsApp link IPC

**Files:**
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-daemon/src/agent/types.rs` (only if event typing/extensions are required)

- [ ] **Step 1: Write failing server tests**

Add tests for:

- each new `ClientMessage::AgentWhatsAppLink*` branch sends correct `DaemonMessage::*`
- subscribe/unsubscribe correctness
- replay order: status snapshot first, then incremental QR/linked/error/disconnected

- [ ] **Step 2: Run targeted server tests and verify failure**

Run:

```bash
cargo test -p tamux-daemon server::tests
```

Expected: missing match arms / unhandled message failures.

- [ ] **Step 3: Implement server match arms**

In `handle_connection` message loop:

- start/stop/status synchronous responses
- subscribe/unsubscribe to runtime event stream
- keep behavior aligned with existing agent event forwarding model

- [ ] **Step 4: Re-run daemon server tests**

Run:

```bash
cargo test -p tamux-daemon server::tests
```

Expected: server tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/types.rs
git commit -m "feat(daemon): handle whatsapp link protocol messages"
```

---

## Task 4: Bridge daemon WhatsApp link API into CLI agent bridge + Electron IPC

**Files:**
- Modify: `crates/amux-cli/src/client.rs`
- Modify: `frontend/electron/main.cjs`
- Modify: `frontend/electron/preload.cjs`
- Modify: `frontend/src/types/amux-bridge.d.ts`
- Update tests: `frontend/electron/main.whatsapp-bridge-config.test.cjs`, `frontend/electron/whatsapp-bridge.test.cjs`

- [ ] **Step 1: Write failing tests for daemon-routed WhatsApp IPC**

Assert:

- `main.cjs` WhatsApp handlers call `sendAgentCommand`/`sendAgentQuery` with new command types (not legacy sidecar RPC)
- `preload.cjs` exposes daemon-backed handlers/events

- [ ] **Step 2: Run failing Electron node tests**

Run:

```bash
node --test frontend/electron/main.whatsapp-bridge-config.test.cjs frontend/electron/whatsapp-bridge.test.cjs
```

Expected: tests fail because handlers still use legacy bridge path.

- [ ] **Step 3: Extend CLI `AgentBridgeCommand` and event mapping**

In `crates/amux-cli/src/client.rs`:

- add new command variants (`whatsapp-link-start`, `whatsapp-link-stop`, `whatsapp-link-status`, `whatsapp-link-subscribe`, `whatsapp-link-unsubscribe`)
- map new `DaemonMessage::*` variants to bridge event payloads used by Electron (`whatsapp-link-status`, `whatsapp-link-qr`, `whatsapp-link-linked`, `whatsapp-link-error`, `whatsapp-link-disconnected`)

- [ ] **Step 4: Migrate Electron main/preload to daemon path**

In `main.cjs`:

- replace `whatsappRpc`/`startWhatsAppBridge` usage in public IPC handlers with `sendAgentCommand`/`sendAgentQuery` to `agent-bridge`
- map bridge events to renderer channels

In `preload.cjs` and `amux-bridge.d.ts`:

- keep API names stable where possible (`whatsappConnect`, `whatsappStatus`, etc.) but back them with daemon protocol.

- [ ] **Step 5: Re-run Electron tests**

Run:

```bash
node --test frontend/electron/main.whatsapp-bridge-config.test.cjs frontend/electron/whatsapp-bridge.test.cjs
```

Expected: tests pass with daemon-routed assertions.

- [ ] **Step 6: Commit**

```bash
git add crates/amux-cli/src/client.rs frontend/electron/main.cjs frontend/electron/preload.cjs frontend/src/types/amux-bridge.d.ts frontend/electron/main.whatsapp-bridge-config.test.cjs frontend/electron/whatsapp-bridge.test.cjs
git commit -m "feat(electron): route whatsapp link via daemon protocol"
```

---

## Task 5: Migrate React Gateway tab to daemon-driven QR/status stream

**Files:**
- Modify: `frontend/src/components/settings-panel/GatewayTab.tsx`
- (Optional helper) Modify: `frontend/src/lib/bridge.ts` only if required by typings

- [ ] **Step 1: Write/update failing component behavior tests (if harness exists) or add deterministic unit checks around mapper logic**

If no reliable component harness exists, add focused pure-function tests for status mapping logic colocated with component utilities.

- [ ] **Step 2: Run frontend checks to establish baseline**

Run:

```bash
cd frontend && npm run build
```

Expected: baseline behavior (note any unrelated pre-existing failures before this task).

- [ ] **Step 3: Replace legacy WhatsApp bridge event wiring**

In `GatewayTab.tsx`:

- consume daemon-backed bridge methods/events introduced in Task 4
- render ASCII QR text block (or preformatted mono block) from daemon payload
- keep existing UX states (`disconnected`, `connecting`, `qr_ready`, `connected`, `error`)

- [ ] **Step 4: Re-run frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: no new TypeScript errors from Gateway tab migration (existing unrelated baseline issues may remain unchanged).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/settings-panel/GatewayTab.tsx
git commit -m "feat(frontend): use daemon whatsapp link stream in gateway tab"
```

---

## Task 6: Implement TUI in-place WhatsApp QR modal workflow

**Files:**
- Modify: `crates/amux-tui/src/state/mod.rs`
- Modify: `crates/amux-tui/src/state/modal.rs`
- Modify: `crates/amux-tui/src/client.rs`
- Modify: `crates/amux-tui/src/app/events.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers.rs`
- Modify: `crates/amux-tui/src/widgets/settings.rs`
- Create: `crates/amux-tui/src/widgets/whatsapp_link.rs` (preferred)

- [ ] **Step 1: Write failing TUI tests**

Add tests for:

- Gateway tab contains a selectable “Link Device” action row
- receiving QR event opens/updates WhatsApp modal with ASCII payload
- connected/error/disconnected events update modal status correctly
- Esc/Cancel sends stop command and closes modal

- [ ] **Step 2: Run targeted TUI tests and verify failure**

Run:

```bash
cargo test -p tamux-tui gateway_tab_mentions_whatsapp
```

Expected: failure for missing actionable row and modal behavior.

- [ ] **Step 3: Add new daemon command + client event plumbing**

Add `DaemonCommand` variants:

- `WhatsAppLinkStart`, `WhatsAppLinkStop`, `WhatsAppLinkStatus`, `WhatsAppLinkSubscribe`, `WhatsAppLinkUnsubscribe`

Add `ClientEvent` variants for typed daemon responses and map them in `client.rs`.

- [ ] **Step 4: Implement modal state and rendering**

- add `ModalKind::WhatsAppLink`
- add dedicated widget rendering for ASCII QR and status text
- integrate keyboard/mouse actions in modal handlers

- [ ] **Step 5: Wire settings action to launch flow**

In Gateway tab behavior:

- selecting “Link Device” sends subscribe + start commands
- do not block regular Cloud API config fields

- [ ] **Step 6: Re-run TUI tests**

Run:

```bash
cargo test -p tamux-tui
```

Expected: TUI tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/state/mod.rs crates/amux-tui/src/state/modal.rs crates/amux-tui/src/client.rs crates/amux-tui/src/app/events.rs crates/amux-tui/src/app/settings_handlers.rs crates/amux-tui/src/app/modal_handlers.rs crates/amux-tui/src/widgets/settings.rs crates/amux-tui/src/widgets/whatsapp_link.rs
git commit -m "feat(tui): add daemon-backed whatsapp qr modal flow"
```

---

## Task 7: Add WhatsApp QR subflow to `tamux setup`

**Files:**
- Modify: `crates/amux-cli/src/setup_wizard.rs`
- Modify: `crates/amux-cli/src/client.rs` (if setup needs helper calls through existing client helpers)
- Modify: `crates/amux-cli/src/main.rs` only if return-action plumbing needs extension

- [ ] **Step 1: Write failing setup wizard tests**

Add tests for:

- gateway chooser includes WhatsApp option
- when WhatsApp chosen, setup enters QR linking subflow
- skip path returns to summary without failure

- [ ] **Step 2: Run setup wizard tests and verify failure**

Run:

```bash
cargo test -p tamux-cli setup_wizard
```

Expected: failures because WhatsApp path does not exist yet.

- [ ] **Step 3: Implement setup WhatsApp gateway option + QR flow**

Implement:

- include WhatsApp in gateway selection list
- on selection: enable gateway + subscribe/start WhatsApp link
- render ASCII QR updates in terminal loop
- finish on linked, or allow skip/cancel with explicit messaging

- [ ] **Step 4: Re-run setup tests**

Run:

```bash
cargo test -p tamux-cli setup_wizard
```

Expected: setup wizard tests pass with new flow.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-cli/src/setup_wizard.rs crates/amux-cli/src/client.rs crates/amux-cli/src/main.rs
git commit -m "feat(cli): add whatsapp qr flow to setup wizard"
```

---

## Task 8: Full verification pass and migration cleanup

**Files:**
- Modify: any touched files from prior tasks for final polish only

- [ ] **Step 1: Run Rust workspace validation**

Run:

```bash
cargo test --workspace
```

Expected: workspace tests pass.

- [ ] **Step 2: Run frontend targeted tests**

Run:

```bash
node --test frontend/electron/main.whatsapp-bridge-config.test.cjs frontend/electron/whatsapp-bridge.test.cjs
```

Expected: daemon-route WhatsApp tests pass.

- [ ] **Step 3: Run frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: no regressions from this phase; if baseline has unrelated failures, record unchanged diagnostics.

- [ ] **Step 4: Manual smoke checks**

Run and verify:

- React: Settings → Gateway → Link Device shows daemon-driven QR and reaches connected state.
- TUI: Gateway tab action opens ASCII QR modal and links.
- Setup: selecting WhatsApp gateway shows QR subflow and respects skip/connect path.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: unify whatsapp qr linking across daemon react tui and setup"
```

---

## Post-implementation cleanup checklist

- Remove legacy Electron-only WhatsApp path after fallback flag removal criteria is met.
- Delete temporary migration flag and dead code in `main.cjs`/`preload.cjs`/bridge typings.
- Re-run full validation before merging removal patch.
