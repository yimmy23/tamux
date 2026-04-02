# Gateway Reconnect Replay Design

## Summary

When tamux reconnects to a previously connected gateway, it should automatically fetch and replay any messages that arrived after the last successfully processed gateway cursor. This recovery must also work across daemon restarts.

The design keeps the current first-connect behavior that avoids replaying old backlog for brand-new gateway connections. Replay is only enabled for gateways that already have persisted reconnect state.

## Goals

- Replay only messages newer than the last successfully processed cursor.
- Persist replay state in daemon-owned storage so reconnect recovery survives daemon restarts.
- Reuse the existing gateway ingestion path so replayed messages behave like live messages.
- Preserve current per-platform bot/self-message filtering.
- Avoid duplicate delivery and preserve oldest-first ordering during replay.

## Non-Goals

- Replaying full historical backlog for first-time gateway connections.
- Changing the agent routing model or gateway thread continuity behavior.
- Building a separate replay subsystem outside the daemon gateway loop.
- Broad refactors unrelated to reconnect recovery.

## Current State

Gateway polling and reconnect behavior is centered in:

- `crates/amux-daemon/src/agent/gateway.rs`
- `crates/amux-daemon/src/agent/gateway_loop.rs`
- `crates/amux-daemon/src/agent/gateway_health.rs`
- `crates/amux-daemon/src/history.rs`

Current behavior is inconsistent across platforms:

- Telegram intentionally skips backlog on first poll via offset `0`.
- Discord intentionally initializes and skips historical messages on first poll.
- Slack keeps in-memory per-channel timestamps and may overlap recent messages after restart.
- WhatsApp relies on daemon-owned native/provider state, but does not yet have a reconnect replay checkpoint integrated with the common gateway recovery story.

Existing deduplication is in-memory and therefore insufficient for restart-safe replay.

## Proposed Architecture

### Persisted Replay Cursors

Store replay cursors in daemon-owned persistence, separate from transient in-memory health state:

- Telegram: global last processed `update_id`
- Slack: per-channel last processed message timestamp
- Discord: per-channel last processed message id
- WhatsApp: per-chat last processed inbound message checkpoint

The persisted model should be scoped by platform plus channel/chat identity so replay decisions are localized and do not cross-contaminate platforms.

### First Connect vs Reconnect

The daemon distinguishes two cases:

1. **First connect with no persisted cursor**
   - preserve current behavior
   - do not replay old backlog
   - initialize cursor from the newest currently visible platform boundary without replay

2. **Reconnect with persisted cursor**
   - fetch messages newer than the cursor
   - replay them through the normal gateway ingestion path
   - advance cursor only after message classification

This keeps first-run noise low while making reconnect recovery reliable.

## Replay Flow

1. Gateway config loads and the platform reconnects.
2. Daemon loads persisted replay cursor(s) for that platform.
3. Replay trigger is explicit per platform family:
   - Telegram, Slack, Discord: start replay once per outage/reconnect cycle when platform health transitions from non-connected to connected
   - WhatsApp: start replay once per outage/reconnect cycle from the native/provider connected event path that marks the link live again
4. Replay runs once for that reconnect cycle before the platform is considered caught up. Later successful polls in the same healthy period do not restart replay unless the platform first transitions back out of connected state.
5. The gateway fetches messages strictly newer than the persisted cursor.
6. The daemon sorts replay batches oldest-first when platform APIs do not already guarantee processing order.
7. Each replayed message passes through the same common ingestion contract used by live messages.
8. A message is considered **classified** when the daemon has deterministically decided one of three outcomes:
   - accepted for normal gateway routing
   - intentionally discarded by an existing self/bot filter
   - intentionally discarded as a duplicate
9. Replay pagination continues until the platform returns no messages newer than the current cursor.
10. The persisted cursor advances only after a message has been classified by the common ingestion contract.

Replay failure must not advance the cursor past an unclassified message. If classification fails on a message, replay stops for that platform and resumes from the last classified cursor on the next poll cycle.

## Platform Notes

### Telegram

Persist the last processed `update_id`. On reconnect, call `getUpdates` from `cursor + 1`. On first-ever startup without a cursor, keep the current no-backlog behavior and establish the initial boundary without replay.

### Slack

Persist per-channel timestamps. On reconnect, query channel history newer than the stored timestamp and replay in oldest-first order. This replaces restart-unsafe in-memory-only continuation.

### Discord

Persist per-channel last processed message id. On reconnect, request messages `after` the persisted id, reverse the API result to oldest-first processing, and replay through the normal path.

### WhatsApp

Persist a reconnect replay checkpoint in daemon-owned WhatsApp state keyed by normalized inbound chat JID. The checkpoint stores the last classified inbound message boundary for that chat, with message id as the primary boundary and provider metadata or timestamp as a transport-specific fallback where needed.

Outbound sends do not create replay cursors. If a self/outbound echo appears as an inbound transport event, it is passed through the normal classifier; once intentionally discarded by existing echo/self filters, it may advance the chat replay cursor so reconnect does not loop on the same ignored event.

## State and Interfaces

The implementation should introduce explicit persistence helpers for gateway replay cursors so the gateway loop does not manipulate raw SQL or ad hoc JSON directly.

Expected responsibilities:

- gateway loop: detect reconnect boundary and orchestrate replay
- platform fetchers: retrieve strictly newer messages for a cursor
- persistence layer: load/save per-platform cursors
- common ingestion path: classify messages, enqueue accepted ones, update thread continuity, and advance persisted cursors for classified live and replayed messages

Replay is a platform-local catch-up phase. After reconnect, that platform keeps fetching and classifying newer messages until it receives an empty page or batch, which marks it as caught up.

## Error Handling

- If replay fetch fails, keep the existing cursor and retry on the next reconnect/poll cycle.
- If one replayed message fails to process, do not skip ahead past it silently.
- Replay must never produce success-shaped fallback behavior that hides failures.
- If persisted cursor data is malformed, surface the error consistently and reset only the affected cursor state, not unrelated gateway state.
- Filtered self/bot messages and replayed duplicates should still advance the cursor boundary when they have been conclusively examined and intentionally discarded, so reconnect loops do not stall on permanently skipped items.

## Duplicate and Ordering Safety

- Replayed messages must be processed oldest-first.
- Existing message id dedup remains in place for same-process overlap protection.
- Persisted cursors are the main restart-safe boundary; they prevent broad recent-window duplicates.
- Platform self/bot filters remain active during replay.
- WhatsApp self-chat replay must not reintroduce assistant echo loops.
- Replay fetches must paginate until caught up so longer disconnects do not silently truncate recovery to a single response page.

## Testing

Add focused coverage for:

- persistence round-trip of each gateway replay cursor shape
- reconnect replay for Telegram, Slack, Discord, and WhatsApp
- restart recovery using persisted cursor state
- first-connect behavior still skipping historical backlog
- duplicate protection when replay overlaps with recently seen in-memory ids
- ordering guarantees for replay batches
- WhatsApp replay preserving current self-echo protections

## Recommended Implementation Direction

Implement cursor persistence first, then wire reconnect-triggered replay into the daemon gateway loop, and finally add per-platform replay fetch logic. This keeps the change incremental and makes it easier to verify correctness before broadening the surface area.
