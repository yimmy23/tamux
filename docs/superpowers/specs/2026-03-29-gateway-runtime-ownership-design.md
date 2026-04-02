# Gateway Runtime Ownership Design

## Summary

`tamux-gateway` should become the only runtime that talks to Slack, Discord, and Telegram. The daemon should stop performing direct gateway transport work and instead supervise the gateway process, exchange structured IPC messages with it, and persist gateway-owned state updates.

This removes the current split-brain design where gateway logic exists in both `crates/amux-gateway` and `crates/amux-daemon`.

## Goals

- Make `tamux-gateway` the single owner of Slack, Discord, and Telegram platform logic.
- Remove direct platform HTTP/polling/send behavior from `amux-daemon`.
- Keep daemon ownership of persistence for gateway cursors, thread bindings, route modes, and health snapshots.
- Make daemon-to-gateway interaction explicit through typed IPC instead of duplicated transport code.
- Make daemon spawning and supervision of `tamux-gateway` the normal lifecycle path.

## Non-Goals

- Reworking WhatsApp native/provider ownership as part of this change.
- Changing agent routing or concierge behavior beyond adapting it to the new gateway IPC path.
- Introducing a second persistence store owned by `tamux-gateway`.
- Rewriting unrelated daemon task/session logic.

## Current State

Gateway behavior is currently split across two runtimes:

- `crates/amux-gateway` contains a standalone gateway process with a generic provider model, but Slack, Discord, and Telegram implementations are explicit stubs.
- `crates/amux-daemon/src/agent/gateway.rs`, `gateway_loop.rs`, `gateway_health.rs`, and gateway-related tool execution paths already contain real platform polling, formatting, send, replay, and health logic.

This creates three problems:

- provider behavior is duplicated conceptually but implemented in only one place
- the standalone gateway binary exists but is not the authoritative gateway runtime
- the daemon violates ownership boundaries by doing platform-specific I/O

## Proposed Architecture

### Runtime Ownership

`tamux-gateway` becomes the only owner of:

- Slack polling and sends
- Discord polling and sends
- Telegram polling and sends
- provider authentication
- per-platform rate limiting
- outbound formatting and chunking
- in-memory cursor state
- in-memory provider health state
- per-channel reply context while the process is running

`amux-daemon` retains only:

- gateway process spawn, shutdown, restart, and supervision
- typed IPC server/client boundary for gateway communication
- persistence of gateway state received from the gateway
- agent/task/session processing for inbound normalized gateway messages

### Boundary Rule

The daemon must not call Slack, Discord, or Telegram HTTP APIs directly after migration. All platform API traffic must originate in `tamux-gateway`.

## IPC Contract

The refactor introduces an explicit gateway IPC protocol between daemon and gateway.

### Gateway To Daemon

`tamux-gateway` sends:

- inbound normalized chat events
- delivery success/failure events for outbound sends
- platform health updates
- replay cursor updates
- thread-binding updates
- route-mode related metadata when needed for daemon continuity

### Daemon To Gateway

`amux-daemon` sends:

- bootstrap configuration for enabled providers
- persisted gateway state needed at startup or restart
- outbound send requests produced by agent tools or gateway workflows
- lifecycle commands such as shutdown or reload

### Bootstrap Model

On startup, the daemon launches `tamux-gateway`, establishes IPC, and sends a bootstrap payload containing:

- enabled gateway providers and credentials/config
- persisted replay cursors
- persisted thread bindings and route metadata required for continuity
- any relevant gateway feature flags

The gateway uses that bootstrap payload to reconstruct runtime state in memory and begin provider polling.

Bootstrap should reuse the existing daemon/gateway IPC transport rather than introducing a second parallel channel.
Provider secrets should continue to originate from daemon-owned config and be passed only as needed for runtime bootstrap, not copied into a new persistent gateway-owned store.

## Data Flow

### Inbound Messages

1. `tamux-gateway` polls a provider.
2. The provider message is normalized into a common gateway event.
3. The gateway sends the event to the daemon over IPC.
4. The daemon processes it through the existing agent routing, concierge, task, and thread continuity flows.
5. When the daemon commits continuity-related state, it persists those updates as it does today.

### Outbound Messages

1. The daemon decides to send a gateway reply.
2. Instead of calling platform HTTP directly, it emits an outbound send request over IPC.
3. `tamux-gateway` formats, chunks, rate-limits, and delivers the message.
4. The gateway reports delivery status and any updated thread context back to the daemon.
5. The daemon persists the resulting state.

### Replay And Health

Replay logic and live provider health evaluation move into `tamux-gateway`, but persistence remains daemon-owned:

- gateway keeps runtime replay/health state in memory
- gateway pushes cursor and health updates to daemon
- daemon persists those updates
- daemon includes persisted state in the next bootstrap payload

This preserves restart safety without forcing the gateway to own a second database.

## Component Changes

### `crates/amux-gateway`

Refactor the gateway binary into the authoritative gateway runtime:

- replace stub provider implementations with real provider adapters
- move shared provider transport logic out of daemon-owned gateway modules
- add IPC client/server integration with typed gateway protocol messages
- own rate limiting, send formatting, replay orchestration, and provider health transitions
- route inbound messages and outbound delivery results through the new IPC boundary

### `crates/amux-daemon`

Refactor the daemon into a gateway supervisor and persistence owner:

- keep `maybe_spawn_gateway` but make it the standard gateway lifecycle path
- add IPC handling for gateway bootstrap, inbound events, outbound send requests, and persistence updates
- remove direct Slack/Discord/Telegram polling and send code from daemon gateway modules once parity is reached
- keep persistence helpers for replay cursors, thread bindings, route modes, and health snapshots
- update gateway tool execution to emit outbound IPC requests instead of direct HTTP calls

### `crates/amux-protocol`

Add explicit message types for:

- gateway bootstrap
- inbound gateway event
- outbound gateway send request
- outbound delivery result
- gateway health update
- gateway cursor update
- gateway thread-binding update

## Migration Plan

The migration should be staged to avoid a broken midpoint.

### Phase 1: Protocol And Bootstrap

- define typed gateway IPC messages
- add daemon supervision and IPC bootstrap path
- add gateway-side runtime state hydration from daemon bootstrap

### Phase 2: Transport Extraction

- move real Slack/Discord/Telegram transport logic behind `tamux-gateway`
- reuse existing daemon implementations by relocating or extracting code instead of re-implementing behavior
- preserve current formatting, replay, and rate-limit semantics during the move

### Phase 3: Daemon Cutover

- switch daemon outbound send tools to IPC-only
- switch inbound polling ownership entirely to `tamux-gateway`
- verify daemon no longer performs direct platform I/O

### Phase 4: Dead Code Removal

- delete daemon-only platform transport code
- remove stale comments and placeholder wording in `crates/amux-gateway`
- simplify daemon gateway modules to supervision, persistence, and event handling only

## Error Handling

- If provider delivery fails, `tamux-gateway` sends structured failure back to the daemon and updates platform health.
- If IPC bootstrap fails, the gateway must not enter a partially active state.
- If the gateway loses IPC connectivity to the daemon, it should fail fast and let daemon supervision restart it.
- If the gateway process exits, the daemon records the failure, marks gateway health accordingly, and respawns with backoff.
- Persistence errors remain daemon-visible because daemon remains the store owner.

## Testing

Add focused coverage for:

- gateway IPC protocol serialization and handling
- daemon bootstrap of gateway state
- inbound event routing from gateway to daemon
- outbound send routing from daemon to gateway
- cursor persistence round-trip through gateway updates and daemon storage
- thread-binding update flow across gateway restart
- gateway health update propagation and restart behavior
- regression coverage proving daemon no longer performs direct Slack/Discord/Telegram HTTP sends

## Recommended Implementation Direction

Implement the IPC contract first, then migrate transport behavior into `tamux-gateway`, then cut the daemon over to IPC-only gateway interactions, and finally remove dead daemon transport code.

That order gives one clear ownership model and prevents the project from keeping two partially authoritative gateway runtimes.
