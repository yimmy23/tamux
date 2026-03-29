# Gateway Runtime Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `tamux-gateway` the single owner of Slack/Discord/Telegram transport logic while `amux-daemon` becomes a supervisor, IPC peer, and persistence owner.

**Architecture:** Reuse the existing `AmuxCodec` daemon socket as the only gateway transport channel. Extend `amux-protocol` with typed gateway bootstrap/event/send/update messages, move real provider transport behavior into `crates/amux-gateway`, then cut daemon polling and outbound sends over to IPC-only paths before deleting direct platform I/O from the daemon.

**Tech Stack:** Rust workspace (`tokio`, `tokio-util`, `serde`, `reqwest`, `tracing`), existing daemon `AmuxCodec` IPC, gateway provider adapters, daemon persistence/history store.

**Spec:** `docs/superpowers/specs/2026-03-29-gateway-runtime-ownership-design.md`

---

## Supervision Policy

- `tamux-gateway` connects to the daemon over the existing `AmuxCodec` transport and fails fast if bootstrap or steady-state IPC breaks.
- The daemon owns restart policy: clean shutdown on daemon stop/config disable, reload on gateway config change, and bounded respawn with backoff on unexpected gateway exit.
- Continuity updates are live messages, not bootstrap-only state. Cursor updates, thread-binding updates, route-mode updates, and health updates must be pushed from gateway to daemon during runtime and persisted immediately.

## Startup Handshake

1. `crates/amux-daemon/src/server.rs` constructs `AgentEngine` and calls `AgentEngine::hydrate()`.
2. `crates/amux-daemon/src/agent/persistence.rs` finishes state hydration, then uses `maybe_spawn_gateway()` as the standard Slack/Discord/Telegram startup path instead of initializing local provider polling.
3. `maybe_spawn_gateway()` owns the child process through `AgentEngine.gateway_process`; daemon runtime state also retains the live framed gateway connection used for outbound send, reload, and shutdown commands.
4. `tamux-gateway` connects over the existing `AmuxCodec` socket and sends a gateway registration message.
5. Daemon responds with a gateway bootstrap message carrying provider credentials/config plus persisted cursor, thread-binding, route-mode, and health-snapshot state. The bootstrap payload becomes the authoritative provider-config path; `maybe_spawn_gateway()` should stop injecting `AMUX_*_TOKEN` env vars as a parallel steady-state configuration mechanism.
6. Gateway acknowledges readiness, starts providers, and begins sending inbound events plus live continuity updates.
7. The same framed socket is reused for the steady-state control plane: send requests, reload, shutdown, delivery results, cursor updates, thread-binding updates, route-mode updates, and health updates.

### Message Direction Table

- `ClientMessage::GatewayRegister`: gateway -> daemon
- `DaemonMessage::GatewayBootstrap`: daemon -> gateway
- `ClientMessage::GatewayAck`: gateway -> daemon
- `ClientMessage::GatewayInboundEvent`: gateway -> daemon
- `ClientMessage::GatewayCursorUpdate`: gateway -> daemon
- `ClientMessage::GatewayThreadBindingUpdate`: gateway -> daemon
- `ClientMessage::GatewayRouteModeUpdate`: gateway -> daemon
- `ClientMessage::GatewayHealthUpdate`: gateway -> daemon
- `ClientMessage::GatewaySendResult`: gateway -> daemon
- `DaemonMessage::GatewaySendRequest`: daemon -> gateway
- `DaemonMessage::GatewayReload`: daemon -> gateway
- `DaemonMessage::GatewayShutdown`: daemon -> gateway

## File Map

### Create

- `crates/amux-gateway/src/ipc.rs`
- `crates/amux-gateway/src/runtime.rs`
- `crates/amux-gateway/src/state.rs`
- `crates/amux-gateway/src/format.rs`
- `crates/amux-gateway/src/health.rs`

### Modify

- `crates/amux-protocol/src/messages.rs`
- `crates/amux-gateway/Cargo.toml`
- `crates/amux-gateway/src/main.rs`
- `crates/amux-gateway/src/router.rs`
- `crates/amux-gateway/src/slack.rs`
- `crates/amux-gateway/src/discord.rs`
- `crates/amux-gateway/src/telegram.rs`
- `crates/amux-daemon/src/server.rs`
- `crates/amux-daemon/src/agent/engine.rs`
- `crates/amux-daemon/src/agent/gateway.rs`
- `crates/amux-daemon/src/agent/gateway_format.rs`
- `crates/amux-daemon/src/agent/gateway_loop.rs`
- `crates/amux-daemon/src/agent/gateway_health.rs`
- `crates/amux-daemon/src/agent/tool_executor.rs`
- `crates/amux-daemon/src/agent/capability_tier.rs`
- `crates/amux-daemon/src/agent/heartbeat_checks.rs`
- `crates/amux-daemon/src/agent/persistence.rs`
- `crates/amux-daemon/src/history.rs`
- `crates/amux-cli/src/client.rs`
- `crates/amux-cli/src/setup_wizard.rs`
- `crates/amux-mcp/src/main.rs`
- `crates/amux-tui/src/client.rs`

### Tests to Add or Update

- `crates/amux-protocol/src/messages.rs`
- `crates/amux-gateway/src/runtime.rs`
- `crates/amux-gateway/src/slack.rs`
- `crates/amux-gateway/src/discord.rs`
- `crates/amux-gateway/src/telegram.rs`
- `crates/amux-daemon/src/server.rs`
- `crates/amux-daemon/src/agent/gateway_loop.rs`
- `crates/amux-daemon/src/agent/tool_executor.rs`
- `crates/amux-daemon/src/agent/heartbeat_checks.rs`
- `crates/amux-cli/src/client.rs`
- `crates/amux-mcp/src/main.rs`
- `crates/amux-tui/src/client.rs`

---

## Task 1: Add typed daemon <-> gateway IPC messages

**Files:**
- Modify: `crates/amux-protocol/src/messages.rs`
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-cli/src/client.rs`
- Modify: `crates/amux-cli/src/setup_wizard.rs`
- Modify: `crates/amux-mcp/src/main.rs`
- Modify: `crates/amux-tui/src/client.rs`
- Test: `crates/amux-protocol/src/messages.rs`

- [ ] **Step 1: Write failing protocol tests**

Add serialization/deserialization tests for new typed gateway messages:

- `gateway_register_round_trip`
- `gateway_bootstrap_round_trip`
- `gateway_ack_round_trip`
- `gateway_incoming_event_round_trip`
- `gateway_send_request_round_trip`
- `gateway_send_result_round_trip`
- `gateway_cursor_update_round_trip`
- `gateway_thread_binding_update_round_trip`
- `gateway_route_mode_update_round_trip`
- `gateway_health_update_round_trip`
- `gateway_reload_command_round_trip`
- `gateway_shutdown_command_round_trip`

- [ ] **Step 2: Run targeted protocol tests and verify failure**

Run:

```bash
cargo test -q -p tamux-protocol gateway_bootstrap_round_trip -- --nocapture
```

Expected: failure because the IPC message variants and payload structs do not exist yet.

- [ ] **Step 3: Add the protocol surface**

Implement explicit protocol structs/enums in `crates/amux-protocol/src/messages.rs` for:

- gateway registration / ready handshake
- gateway bootstrap payload
- gateway inbound normalized message
- gateway outbound send request
- gateway delivery result with a correlation id matching the outbound request
- gateway cursor update
- gateway thread-binding update
- gateway route-mode update
- gateway health update
- gateway reload command
- gateway shutdown command

Add matching `ClientMessage` / `DaemonMessage` variants so the gateway can connect over the existing daemon socket without spawning a fake PTY session.
Document sender/receiver direction in comments next to each new protocol variant so the handshake sequence is explicit in code, not only in the plan.
Include gateway feature flags in the bootstrap payload alongside provider credentials/config and persisted continuity state.
Update exhaustive `ClientMessage` / `DaemonMessage` consumers in CLI, MCP, and TUI immediately after adding the new variants so later tasks are not blocked by workspace compile failures.
Update the daemon-side exhaustive protocol handling in `crates/amux-daemon/src/server.rs` in the same patch so the workspace remains buildable after the protocol change lands.

- [ ] **Step 4: Re-run targeted protocol tests**

Run:

```bash
cargo test -q -p tamux-protocol gateway_bootstrap_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_register_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_ack_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_incoming_event_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_send_request_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_send_result_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_cursor_update_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_thread_binding_update_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_route_mode_update_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_health_update_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_reload_command_round_trip -- --nocapture
cargo test -q -p tamux-protocol gateway_shutdown_command_round_trip -- --nocapture
rg -n "ClientMessage::|DaemonMessage::" crates/amux-daemon crates/amux-cli crates/amux-mcp crates/amux-tui -g '*.rs'
cargo check -p tamux-daemon
cargo check -p tamux-gateway
cargo check -p tamux-cli
cargo check -p tamux-mcp
cargo check -p tamux-tui
```

Expected: pass.

- [ ] **Step 5: Commit protocol changes**

Run:

```bash
git add crates/amux-protocol/src/messages.rs crates/amux-daemon/src/server.rs crates/amux-cli/src/client.rs crates/amux-cli/src/setup_wizard.rs crates/amux-mcp/src/main.rs crates/amux-tui/src/client.rs
git commit -m "feat: add typed gateway ipc messages"
```

Expected: one commit containing only protocol message additions and tests.

---

## Task 2: Build the gateway runtime shell around the new IPC contract

**Files:**
- Create: `crates/amux-gateway/src/ipc.rs`
- Create: `crates/amux-gateway/src/runtime.rs`
- Create: `crates/amux-gateway/src/state.rs`
- Modify: `crates/amux-gateway/src/main.rs`
- Modify: `crates/amux-gateway/Cargo.toml`
- Test: `crates/amux-gateway/src/runtime.rs`

- [ ] **Step 1: Write failing runtime tests**

Add tests for:

- `gateway_runtime_bootstraps_from_daemon_message`
- `gateway_runtime_routes_incoming_event_to_daemon_channel`
- `gateway_runtime_applies_outbound_send_request_to_provider_queue`
- `gateway_runtime_emits_live_cursor_thread_binding_and_route_mode_updates`

- [ ] **Step 2: Run targeted gateway runtime tests and verify failure**

Run:

```bash
cargo test -q -p tamux-gateway gateway_runtime_bootstraps_from_daemon_message -- --nocapture
```

Expected: failure because there is no `ipc` or `runtime` module and `main.rs` still spawns a PTY session instead of using a typed gateway session.
Expected: failure because there is no `ipc` or `runtime` module and `main.rs` still uses the old daemon-session command flow rather than a gateway registration/bootstrap handshake.

- [ ] **Step 3: Implement the runtime shell**

Implement:

- `ipc.rs` to connect to the daemon socket using `AmuxCodec`
- `state.rs` to hold gateway runtime state hydrated from daemon bootstrap
- `runtime.rs` to own the main select loop, incoming provider channel, outbound request channel, and daemon IPC stream
- gateway-side handshake sequence: connect, register, receive bootstrap, send ready ack, then start providers
- replace `SlackProvider::from_env()`, `DiscordProvider::from_env()`, and `TelegramProvider::from_env()` with bootstrap-backed constructors or runtime config injection so authenticated providers are built from daemon-supplied bootstrap data before `GatewayRuntime` starts
- move the existing gateway event loop and provider orchestration out of `main.rs` into `runtime.rs`
- `main.rs` as a thin bootstrap only: load logging, obtain bootstrap config, build providers, and run `GatewayRuntime`

Replace the current daemon-session command flow in `crates/amux-gateway/src/main.rs` with the new gateway registration/bootstrap handshake.

- [ ] **Step 4: Re-run targeted gateway runtime tests**

Run:

```bash
cargo test -q -p tamux-gateway gateway_runtime_bootstraps_from_daemon_message -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_routes_incoming_event_to_daemon_channel -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_applies_outbound_send_request_to_provider_queue -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_emits_live_cursor_thread_binding_and_route_mode_updates -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit runtime-shell changes**

Run:

```bash
git add crates/amux-gateway/Cargo.toml crates/amux-gateway/src/main.rs crates/amux-gateway/src/ipc.rs crates/amux-gateway/src/runtime.rs crates/amux-gateway/src/state.rs
git commit -m "refactor: add typed gateway runtime shell"
```

Expected: one commit containing only the standalone gateway runtime scaffolding and tests.

---

## Task 3: Move Slack, Discord, and Telegram transport behavior into `tamux-gateway`

**Files:**
- Create: `crates/amux-gateway/src/format.rs`
- Create: `crates/amux-gateway/src/health.rs`
- Modify: `crates/amux-gateway/Cargo.toml`
- Modify: `crates/amux-gateway/src/slack.rs`
- Modify: `crates/amux-gateway/src/discord.rs`
- Modify: `crates/amux-gateway/src/telegram.rs`
- Modify: `crates/amux-gateway/src/router.rs`
- Modify: `crates/amux-daemon/src/agent/gateway.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_format.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_health.rs`
- Test: `crates/amux-gateway/src/slack.rs`
- Test: `crates/amux-gateway/src/discord.rs`
- Test: `crates/amux-gateway/src/telegram.rs`

- [ ] **Step 1: Write failing provider tests**

Add focused tests for:

- `slack_provider_connects_and_posts_messages_via_http_client`
- `discord_provider_polls_and_filters_bot_messages`
- `telegram_provider_long_poll_updates_cursor_and_sends_replies`
- `gateway_router_normalizes_messages_without_daemon_command_routing`

Use fake HTTP responses or local test helpers so the tests do not require live provider credentials.

- [ ] **Step 2: Run targeted provider tests and verify failure**

Run:

```bash
cargo test -q -p tamux-gateway slack_provider_connects_and_posts_messages_via_http_client -- --nocapture
cargo test -q -p tamux-gateway discord_provider_polls_and_filters_bot_messages -- --nocapture
cargo test -q -p tamux-gateway telegram_provider_long_poll_updates_cursor_and_sends_replies -- --nocapture
```

Expected: failures because provider modules are still explicit stubs and do not own rate limiting, formatting, or cursor behavior.

- [ ] **Step 3: Port real transport logic into the gateway crate**

Move or adapt the existing daemon implementations so `crates/amux-gateway` owns:

- Slack send/poll behavior now living in daemon gateway code
- Discord send/poll behavior now living in daemon gateway code
- Telegram long-poll/send behavior now living in daemon gateway code
- outbound formatting/chunking rules now living in `crates/amux-daemon/src/agent/gateway_format.rs`
- provider health and rate-limit behavior now living in daemon gateway health helpers
- `reqwest` and any transport dependencies required by the moved provider code

Leave `gateway_format.rs` only as a temporary compatibility layer during this task; the end state is that Slack/Discord/Telegram formatting lives in `crates/amux-gateway/src/format.rs`.
Reduce `crates/amux-gateway/src/router.rs` to normalization-only behavior. After the migration, the gateway should normalize inbound provider messages and pass them to the daemon; command-prefix interpretation, task routing, and managed-command decisions live in daemon-side routing flows only.

Keep daemon behavior intact during this task by moving logic incrementally rather than deleting daemon code first.

- [ ] **Step 4: Re-run targeted provider tests**

Run:

```bash
cargo test -q -p tamux-gateway slack_provider_connects_and_posts_messages_via_http_client -- --nocapture
cargo test -q -p tamux-gateway discord_provider_polls_and_filters_bot_messages -- --nocapture
cargo test -q -p tamux-gateway telegram_provider_long_poll_updates_cursor_and_sends_replies -- --nocapture
cargo test -q -p tamux-gateway gateway_router_normalizes_messages_without_daemon_command_routing -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit provider transport move**

Run:

```bash
git add crates/amux-gateway/Cargo.toml crates/amux-gateway/src/format.rs crates/amux-gateway/src/health.rs crates/amux-gateway/src/router.rs crates/amux-gateway/src/slack.rs crates/amux-gateway/src/discord.rs crates/amux-gateway/src/telegram.rs crates/amux-daemon/src/agent/gateway.rs crates/amux-daemon/src/agent/gateway_format.rs crates/amux-daemon/src/agent/gateway_health.rs
git commit -m "feat: move gateway provider transport into tamux-gateway"
```

Expected: one commit containing the real provider behavior plus any temporary compatibility extraction in daemon modules.

---

## Task 4: Cut the daemon over to gateway supervision, bootstrap, and inbound IPC

**Files:**
- Modify: `crates/amux-daemon/src/server.rs`
- Modify: `crates/amux-daemon/src/agent/engine.rs`
- Modify: `crates/amux-daemon/src/agent/config.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_loop.rs`
- Modify: `crates/amux-daemon/src/agent/persistence.rs`
- Modify: `crates/amux-daemon/src/history.rs`
- Modify: `crates/amux-daemon/src/agent/capability_tier.rs`
- Test: `crates/amux-daemon/src/server.rs`
- Test: `crates/amux-daemon/src/agent/gateway_loop.rs`

- [ ] **Step 1: Write failing daemon IPC tests**

Add tests for:

- `gateway_bootstrap_uses_persisted_cursor_and_thread_state`
- `gateway_bootstrap_restores_health_snapshots`
- `gateway_incoming_ipc_event_enqueues_agent_processing_without_poll_loop`
- `daemon_respawns_gateway_process_when_enabled`
- `daemon_gateway_reload_requests_clean_restart`
- `daemon_gateway_restart_backoff_applies_after_ipc_loss`
- `daemon_disables_local_gateway_polling_when_standalone_gateway_is_enabled`
- `gateway_config_reload_uses_spawn_restart_path_not_init_gateway`

- [ ] **Step 2: Run targeted daemon tests and verify failure**

Run:

```bash
cargo test -q -p tamux-daemon gateway_bootstrap_uses_persisted_cursor_and_thread_state -- --nocapture
cargo test -q -p tamux-daemon gateway_incoming_ipc_event_enqueues_agent_processing_without_poll_loop -- --nocapture
```

Expected: failures because the daemon still initializes local gateway polling state and does not accept typed gateway IPC events.

- [ ] **Step 3: Implement daemon supervision and inbound cutover**

Implement:

- daemon-side handling for the new gateway IPC messages in `server.rs`
- replace the `hydrate() -> init_gateway()` startup path with `hydrate() -> maybe_spawn_gateway()` for Slack/Discord/Telegram ownership
- bootstrap payload assembly from persisted gateway state
- bootstrap payload as the sole provider credential/config path for `tamux-gateway`
- bootstrap payload coverage for route-mode, thread-binding, and health-snapshot metadata that must survive gateway restart
- engine/runtime state that tracks gateway connection health without owning live provider clients
- `AgentEngine.gateway_process` as the owner of the child process and an `AgentEngine`-owned shared gateway IPC sender/handle that other daemon modules can enqueue send/reload/shutdown requests through without reaching into `server.rs`
- `maybe_spawn_gateway` as the standard enabled-gateway lifecycle path on startup, config enable, and config reload
- removal of `AMUX_*_TOKEN` env injection from `maybe_spawn_gateway()` once typed bootstrap is in place
- explicit lifecycle control handling for gateway shutdown and reload commands
- update the config mutation path in `crates/amux-daemon/src/agent/config.rs` so `set_config_item_json()`, `merge_config_patch_json()`, and `reinit_gateway()` restart the standalone gateway path rather than falling back to `init_gateway()`
- bounded restart backoff policy after unexpected exit or IPC loss
- inbound gateway message handling that reuses existing agent routing without local provider polling
- disable only the Slack/Discord/Telegram portion of `gateway_tick -> poll_gateway_messages()` as part of this cutover when standalone gateway ownership is enabled, while preserving current WhatsApp behavior because WhatsApp ownership is out of scope
- daemon-owned persistence helpers and history/schema updates for gateway health snapshots so runtime health can be restored on restart
- do not broaden WhatsApp ownership in this task; only make a minimal compile-only compatibility edit in `whatsapp_native.rs` later if the new gateway snapshot types force it

- [ ] **Step 4: Re-run targeted daemon tests**

Run:

```bash
cargo test -q -p tamux-daemon gateway_bootstrap_uses_persisted_cursor_and_thread_state -- --nocapture
cargo test -q -p tamux-daemon gateway_bootstrap_restores_health_snapshots -- --nocapture
cargo test -q -p tamux-daemon gateway_incoming_ipc_event_enqueues_agent_processing_without_poll_loop -- --nocapture
cargo test -q -p tamux-daemon daemon_respawns_gateway_process_when_enabled -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_reload_requests_clean_restart -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_restart_backoff_applies_after_ipc_loss -- --nocapture
cargo test -q -p tamux-daemon daemon_disables_local_gateway_polling_when_standalone_gateway_is_enabled -- --nocapture
cargo test -q -p tamux-daemon gateway_config_reload_uses_spawn_restart_path_not_init_gateway -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit daemon supervision cutover**

Run:

```bash
git add crates/amux-daemon/src/server.rs crates/amux-daemon/src/agent/engine.rs crates/amux-daemon/src/agent/config.rs crates/amux-daemon/src/agent/gateway_loop.rs crates/amux-daemon/src/agent/persistence.rs crates/amux-daemon/src/history.rs crates/amux-daemon/src/agent/capability_tier.rs
git commit -m "refactor: make daemon supervise standalone gateway runtime"
```

Expected: one commit containing the daemon bootstrap/inbound supervision changes.

---

## Task 5: Cut outbound gateway sends over to IPC and preserve persistence updates

**Files:**
- Modify: `crates/amux-daemon/src/agent/tool_executor.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_loop.rs`
- Modify: `crates/amux-daemon/src/agent/heartbeat_checks.rs`
- Modify: `crates/amux-daemon/src/server.rs`
- Test: `crates/amux-daemon/src/agent/tool_executor.rs`
- Test: `crates/amux-daemon/src/agent/heartbeat_checks.rs`

- [ ] **Step 1: Write failing outbound-send tests**

Add tests for:

- `send_slack_message_emits_gateway_ipc_request`
- `send_discord_message_emits_gateway_ipc_request`
- `send_telegram_message_emits_gateway_ipc_request`
- `heartbeat_checks_read_gateway_health_from_ipc_updates`

- [ ] **Step 2: Run targeted outbound tests and verify failure**

Run:

```bash
cargo test -q -p tamux-daemon send_slack_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon send_discord_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon send_telegram_message_emits_gateway_ipc_request -- --nocapture
```

Expected: failures because `tool_executor.rs` still performs direct provider HTTP calls.

- [ ] **Step 3: Replace direct platform sends with IPC requests**

Implement:

- `tool_executor` gateway send tools as IPC request emitters that attach a correlation id to each `GatewaySendRequest` and wait for the matching `GatewaySendResult` before returning, preserving current synchronous tool semantics
- persistence updates driven by gateway delivery-result / thread-binding / cursor / health messages
- heartbeat and capability-tier consumers reading daemon-owned gateway snapshots instead of live provider state structs

Do not leave any direct Slack/Discord/Telegram HTTP calls in daemon send paths.

- [ ] **Step 4: Re-run targeted outbound tests**

Run:

```bash
cargo test -q -p tamux-daemon send_slack_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon send_discord_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon send_telegram_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon heartbeat_checks_read_gateway_health_from_ipc_updates -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit outbound IPC cutover**

Run:

```bash
git add crates/amux-daemon/src/agent/tool_executor.rs crates/amux-daemon/src/agent/gateway_loop.rs crates/amux-daemon/src/agent/heartbeat_checks.rs crates/amux-daemon/src/server.rs
git commit -m "refactor: route gateway sends through tamux-gateway ipc"
```

Expected: one commit containing only outbound cutover and dependent snapshot consumers.

---

## Task 6: Remove dead daemon transport code and close the feature gap

**Files:**
- Modify: `crates/amux-daemon/src/agent/gateway.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_format.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_health.rs`
- Modify: `crates/amux-daemon/src/agent/gateway_loop.rs`
- Modify: `crates/amux-gateway/src/main.rs`
- Modify: `crates/amux-gateway/src/slack.rs`
- Modify: `crates/amux-gateway/src/discord.rs`
- Modify: `crates/amux-gateway/src/telegram.rs`
- Test: `crates/amux-daemon/src/agent/gateway_loop.rs`
- Test: `crates/amux-gateway/src/runtime.rs`

- [ ] **Step 1: Write failing cleanup/regression tests**

Add regression coverage for:

- `daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram`
- `daemon_gateway_send_path_no_longer_issues_platform_http_requests`
- `gateway_runtime_delivers_outbound_response_to_origin_provider`
- `gateway_process_full_round_trip_uses_single_transport_owner`
- `gateway_state_updates_survive_gateway_restart`
- `gateway_health_snapshots_survive_gateway_restart`

- [ ] **Step 2: Run targeted cleanup tests and verify failure**

Run:

```bash
cargo test -q -p tamux-daemon daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_send_path_no_longer_issues_platform_http_requests -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_delivers_outbound_response_to_origin_provider -- --nocapture
cargo test -q -p tamux-daemon gateway_state_updates_survive_gateway_restart -- --nocapture
cargo test -q -p tamux-daemon gateway_health_snapshots_survive_gateway_restart -- --nocapture
```

Expected: failure until daemon poll ownership and response-routing placeholders are fully removed.

- [ ] **Step 3: Delete dead transport paths and placeholders**

Implement:

- remove daemon-owned Slack/Discord/Telegram polling/send code that is no longer used
- remove or deprecate daemon-owned Slack/Discord/Telegram formatting helpers in `gateway_format.rs` once `crates/amux-gateway/src/format.rs` is authoritative
- reduce daemon gateway modules to supervision, persistence orchestration, and event handling
- remove `tamux-gateway` placeholder response-delivery path and any remaining provider stub comments
- keep only one authoritative transport implementation in `crates/amux-gateway`

- [ ] **Step 4: Re-run targeted cleanup tests**

Run:

```bash
cargo test -q -p tamux-daemon daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_send_path_no_longer_issues_platform_http_requests -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_delivers_outbound_response_to_origin_provider -- --nocapture
cargo test -q -p tamux-gateway gateway_process_full_round_trip_uses_single_transport_owner -- --nocapture
cargo test -q -p tamux-daemon gateway_state_updates_survive_gateway_restart -- --nocapture
cargo test -q -p tamux-daemon gateway_health_snapshots_survive_gateway_restart -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit dead-code removal**

Run:

```bash
git add crates/amux-daemon/src/agent/gateway.rs crates/amux-daemon/src/agent/gateway_format.rs crates/amux-daemon/src/agent/gateway_health.rs crates/amux-daemon/src/agent/gateway_loop.rs crates/amux-gateway/src/main.rs crates/amux-gateway/src/slack.rs crates/amux-gateway/src/discord.rs crates/amux-gateway/src/telegram.rs
git commit -m "refactor: remove daemon-owned gateway transport paths"
```

Expected: one commit containing only cleanup and end-state ownership enforcement.

---

## Task 7: Full verification and operator smoke checks

**Files:**
- Modify: only as needed from previous tasks

- [ ] **Step 1: Run focused Rust verification**

Run:

```bash
cargo test -q -p tamux-protocol gateway_bootstrap_round_trip -- --nocapture
cargo test -q -p tamux-gateway gateway_runtime_bootstraps_from_daemon_message -- --nocapture
cargo test -q -p tamux-gateway slack_provider_connects_and_posts_messages_via_http_client -- --nocapture
cargo test -q -p tamux-gateway discord_provider_polls_and_filters_bot_messages -- --nocapture
cargo test -q -p tamux-gateway telegram_provider_long_poll_updates_cursor_and_sends_replies -- --nocapture
cargo test -q -p tamux-daemon gateway_bootstrap_uses_persisted_cursor_and_thread_state -- --nocapture
cargo test -q -p tamux-daemon gateway_bootstrap_restores_health_snapshots -- --nocapture
cargo test -q -p tamux-daemon send_slack_message_emits_gateway_ipc_request -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram -- --nocapture
cargo test -q -p tamux-daemon daemon_gateway_send_path_no_longer_issues_platform_http_requests -- --nocapture
cargo test -q -p tamux-daemon gateway_state_updates_survive_gateway_restart -- --nocapture
cargo test -q -p tamux-daemon gateway_health_snapshots_survive_gateway_restart -- --nocapture
```

Expected: pass.

- [ ] **Step 2: Run broader workspace verification**

Run:

```bash
cargo test --workspace
cargo check --workspace
```

Expected: full workspace test and compile success.

- [ ] **Step 3: Manual smoke-check list**

Verify manually:

- gateway bootstrap succeeds and providers start from daemon-supplied bootstrap data
- daemon startup spawns `tamux-gateway` when gateway is enabled
- disabling gateway stops spawn/restart behavior cleanly
- daemon no longer polls Slack/Discord/Telegram directly once standalone gateway ownership is enabled
- inbound Slack/Discord/Telegram messages reach the daemon through the new IPC path
- outbound agent replies are delivered by `tamux-gateway`, not by daemon-owned HTTP calls
- replay cursor and thread-binding updates survive gateway restart because daemon persistence rehydrates bootstrap
- gateway health still appears correctly in operator-facing status surfaces

- [ ] **Step 4: Final commit if verification required follow-up fixes**

Run only if Step 3 required final corrections:

```bash
git add -A
git commit -m "test: finish gateway ownership verification fixes"
```

Expected: no-op if prior commits were sufficient.
