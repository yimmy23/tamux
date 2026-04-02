# Daemon-Owned WhatsApp QR Unification Design

## Problem

WhatsApp device linking is currently fragmented:

- React uses an Electron-side bridge path that recently broke (`ERR_REQUIRE_ESM`) and leaked irrelevant GPU stderr noise.
- TUI has no in-place QR linking workflow.
- `tamux setup` does not provide a WhatsApp QR linking step.

This creates inconsistent behavior across surfaces and makes onboarding/linking unreliable.

## Goal

Make WhatsApp QR linking daemon-owned and protocol-driven so React, TUI, and setup use one backend and one state model.

## Scope

In scope:

- New daemon WhatsApp link runtime (sidecar manager) as source of truth.
- New typed protocol messages/events for start/stop/status/QR/linked/error.
- TUI QR modal with ASCII QR rendering and auto-refresh updates.
- Setup wizard WhatsApp option + QR flow when gateway WhatsApp is enabled.
- React migration from Electron IPC WhatsApp bridge to daemon protocol stream.

Out of scope:

- Replacing existing WhatsApp Cloud API send path.
- Full Rust-native WhatsApp implementation (no JS sidecar).

## Architecture

### 1) Daemon runtime: `whatsapp_link`

Add a daemon-owned runtime component that:

- starts/stops/monitors a WhatsApp sidecar process,
- owns current link state,
- streams typed events to subscribed clients,
- retries QR refresh automatically until connected.

State machine (high-level):

- `disconnected`
- `starting`
- `qr_ready`
- `connected`
- `error`

The daemon remains authoritative; clients are renderers + intent senders.

### 2) Protocol surface (typed, not generic JSON)

Add explicit `ClientMessage` and `DaemonMessage` variants.

Client requests:

- `AgentWhatsAppLinkStart`
- `AgentWhatsAppLinkStop`
- `AgentWhatsAppLinkStatus`
- `AgentWhatsAppLinkSubscribe`
- `AgentWhatsAppLinkUnsubscribe`

Daemon responses/events:

- `AgentWhatsAppLinkStatus { state, phone, last_error }`
- `AgentWhatsAppLinkQr { ascii_qr, expires_at_ms }`
- `AgentWhatsAppLinked { phone }`
- `AgentWhatsAppLinkError { message, recoverable }`
- `AgentWhatsAppLinkDisconnected { reason }`

This avoids ad-hoc parsing and stabilizes all clients against one contract.

### 3) Sidecar ownership

Reuse the current JS bridge logic, but move process lifecycle ownership into daemon runtime (instead of Electron main).

Key constraints:

- daemon-spawned process uses node-compatible mode and ESM-safe loading,
- stderr is normalized into actionable link errors,
- no raw GPU process noise is surfaced to users.

## UX/Data Flow

### TUI flow

1. User triggers `Link Device` in Gateway settings.
2. TUI opens `WhatsAppLink` modal and sends `AgentWhatsAppLinkStart`.
3. Daemon emits `AgentWhatsAppLinkQr` updates.
4. Modal renders ASCII QR in-place; QR auto-refreshes until connected.
5. On `AgentWhatsAppLinked`, modal shows success and exits to settings.
6. On recoverable error, modal stays open and continues refresh loop.

### Setup wizard flow

1. Gateway step includes WhatsApp option.
2. If gateway WhatsApp is enabled, setup enters QR-link subflow.
3. Setup subscribes to daemon WhatsApp link events and renders ASCII QR in terminal.
4. User can wait for connect or skip.

### React flow

Replace Electron WhatsApp bridge handlers in Gateway tab with daemon protocol calls + event subscription.

Result: React/TUI/setup all consume the same daemon stream.

## Error Handling

- Sidecar spawn/import/start failures emit `AgentWhatsAppLinkError` with clear remediation text.
- QR expiration is treated as normal lifecycle; daemon emits fresh QR without forcing manual retry.
- Disconnect transitions emit typed disconnected reason.
- Daemon retains last known status for new subscribers.

No silent fallbacks; all failures are explicit and surfaced consistently.

## Testing Plan

### Protocol

- serde roundtrip tests for all new message variants.
- backward-compat checks for unchanged existing protocol variants.

### Daemon runtime

- unit tests for state transitions (`start -> qr_ready -> connected`, error paths, stop).
- retry/auto-refresh tests for QR regeneration loop.
- subscriber behavior tests (new subscriber receives latest status).

### TUI

- modal render test for ASCII QR content.
- event handling tests for start/qr/linked/error transitions.
- keyboard tests for cancel/close behavior.

### Setup wizard

- gateway option includes WhatsApp path.
- conditional WhatsApp QR step only when gateway WhatsApp is enabled.
- skip/continue behavior tests.

### React

- Gateway tab integration tests with daemon-driven QR/status events.

## Rollout Notes

- Keep old Electron bridge path behind a temporary fallback flag during migration window.
- Remove fallback once daemon protocol path is validated across all surfaces.

## Risks & Mitigations

- **Risk:** sidecar lifecycle complexity in daemon.
  - **Mitigation:** small dedicated runtime module with explicit state machine + tests.
- **Risk:** client drift during migration.
  - **Mitigation:** typed protocol contract + parity tests in TUI/setup/react.
- **Risk:** terminal QR readability variance.
  - **Mitigation:** fixed-width ASCII QR renderer with minimum size and quiet-zone checks.

## Acceptance Criteria

- React WhatsApp linking works without Electron-owned bridge path.
- TUI can link WhatsApp in-place via ASCII QR modal.
- `tamux setup` can run WhatsApp QR linking flow when gateway WhatsApp is enabled.
- QR refreshes automatically until connected.
- All three surfaces use the same daemon typed protocol.
