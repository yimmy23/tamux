# Codebase Concerns

**Analysis Date:** 2026-03-22

---

## Tech Debt

**Gateway integrations are stubs only (Discord, Slack, Telegram in daemon crate):**
- Issue: `crates/amux-gateway/src/discord.rs`, `slack.rs`, `telegram.rs` compile and register providers, but every method (`connect`, `recv`, `send`) is a no-op returning `Ok(())`. If users configure gateway tokens in the daemon, the daemon-side gateway binary does nothing.
- Files: `crates/amux-gateway/src/discord.rs`, `crates/amux-gateway/src/slack.rs`, `crates/amux-gateway/src/telegram.rs`
- Impact: Gateway messages are silently dropped in the daemon path. The Electron main process (`frontend/electron/main.cjs`) has working Slack/Telegram/Discord polling, so the real gateway is frontend-only. The `amux-gateway` crate is misleadingly named and uncompleted.
- Fix approach: Either delete `amux-gateway` crate and mark it as not-yet-implemented, or wire in actual HTTP/WebSocket client code for each platform.

**`AnalyzeSession` handler returns raw text with no AI processing:**
- Issue: The `ClientMessage::AnalyzeSession` branch in `crates/amux-daemon/src/server.rs:585-601` has an explicit TODO: "Send to AI model. For now, return the raw text." It just echoes back scrollback buffer content without any LLM analysis.
- Files: `crates/amux-daemon/src/server.rs:588`
- Impact: Any feature relying on session analysis (e.g., auto-title, anomaly detection) gets unprocessed terminal output, not intelligence.
- Fix approach: Pipe the text through `AgentEngine::send_message_inner` with a summarization prompt, or remove the handler until the feature is designed.

**Circuit breaker built but not wired into LLM call path:**
- Issue: `crates/amux-daemon/src/agent/circuit_breaker.rs` is a complete, well-structured circuit breaker implementation. The module-level comment reads: "Not yet wired into the LLM call path — infrastructure ready for integration."
- Files: `crates/amux-daemon/src/agent/circuit_breaker.rs`, `crates/amux-daemon/src/agent/mod.rs:21` (`#[allow(dead_code)]` on the import)
- Impact: LLM API outages cause cascading retry storms across all concurrent agent tasks. The retry logic in `crates/amux-daemon/src/agent/llm_client.rs:500-540` runs unbounded without circuit breaking.
- Fix approach: Instantiate one `CircuitBreaker` per provider in `AgentEngine`, gate `send_completion_request` calls through it.

**TUI input uses custom Normal/Insert vim modal system despite planned removal:**
- Issue: `crates/amux-tui/src/state/input.rs` defines `InputMode::Normal` and `InputMode::Insert`. The TUI feedback backlog (memory note `tui_feedback_2026-03-19.md`) specifically calls out "Remove Normal/Insert modes — user doesn't understand vim modes; should always be in Insert mode." The code still ships with mode toggling.
- Files: `crates/amux-tui/src/state/input.rs`, `crates/amux-tui/src/app/keyboard.rs`
- Impact: Confusing UX for users unfamiliar with modal editing. Breaks discoverability.
- Fix approach: Remove `InputMode::Normal` variant, delete all mode-switch key handlers, default always to `Insert`. The memory note suggests replacing with `tui-textarea` crate for full cursor/editing support.

**`set_layout_preset`, `equalize_layout`, `run_snippet` workspace tool handlers are empty TODOs:**
- Issue: In `frontend/src/components/agent-chat-panel/runtime.tsx:570-577`, three `WorkspaceCommand` cases are `break; // TODO` with no implementation.
- Files: `frontend/src/components/agent-chat-panel/runtime.tsx:570-577`
- Impact: If the agent calls `set_layout_preset`, `equalize_layout`, or `run_snippet`, the command is silently dropped. No error is surfaced to the agent or user.
- Fix approach: Either implement the handlers or emit an error event so the agent can recover.

**Workspace/pane management tools are advertised to TUI-connected daemon sessions:**
- Issue: `crates/amux-daemon/src/agent/tool_executor.rs:593` — workspace tools (`create_workspace`, `create_surface`, `create_pane`, `set_active_workspace`, etc.) are included in the tool list for daemon-mode agents. These tools work by emitting `WorkspaceCommand` events that the Electron frontend handles. When the agent is invoked from the TUI (which has no Electron bridge), the event is emitted but never consumed.
- Files: `crates/amux-daemon/src/agent/tool_executor.rs:593-940`, `crates/amux-daemon/src/agent/types.rs:1455-1488`
- Impact: Agent calls workspace tools, waits for a response that never arrives (or silently fails). There is no `ToolsConfig` flag to disable workspace tools at the config level.
- Fix approach: Add `workspace_management: bool` to `ToolsConfig`, default `false`, set `true` only when an Electron client subscribes.

**Multiple independent `HistoryStore::new()` calls open separate SQLite connections to the same database:**
- Issue: `HistoryStore::new()` is called at three call sites: `crates/amux-daemon/src/session_manager.rs:37`, `crates/amux-daemon/src/agent/engine.rs:143`, and `crates/amux-daemon/src/snapshot.rs:883`. Each call opens a new connection, and `open_connection()` creates a fresh connection per operation (not pooled). No WAL mode is configured.
- Files: `crates/amux-daemon/src/history.rs:2451-2455`, `crates/amux-daemon/src/session_manager.rs:37`, `crates/amux-daemon/src/agent/engine.rs:143`
- Impact: Concurrent writes from different owners (session manager vs. agent engine) risk locking contention. SQLite defaults to DELETE journal mode without WAL; this creates writer-blocks-all-readers conditions under high agent activity.
- Fix approach: Enable WAL mode in `open_connection()` via `PRAGMA journal_mode=WAL`, and share a single `HistoryStore` instance via `Arc` rather than constructing independently.

---

## Known Bugs

**TUI chat overscroll shows black screen:**
- Symptoms: Scrolling past the top of the chat renders an empty/black area instead of clamping at the first message.
- Files: `crates/amux-tui/src/widgets/chat.rs`, `crates/amux-tui/src/state/chat.rs`
- Trigger: Scroll up aggressively in any chat with few messages.
- Workaround: Scroll down to recover. The `resolved_scroll` function at line 985 computes `scroll.min(max_scroll)` but off-by-one in `visible_window_bounds` can still leave a gap when `max_scroll == 0`.

**Chat not fully scrolled to bottom on new message arrival:**
- Symptoms: The last line of a new message is clipped at the bottom edge until the next message arrives.
- Files: `crates/amux-tui/src/widgets/chat.rs`, `crates/amux-tui/src/state/chat.rs`
- Trigger: Receive any streaming assistant message.
- Workaround: None; resolves itself on next render tick.

**Reasoning blocks appear after tool calls in TUI chat rendering:**
- Symptoms: The thinking/reasoning for a turn is shown below the tool call blocks it preceded, reversing the actual LLM reasoning order.
- Files: `crates/amux-tui/src/widgets/chat.rs`, `crates/amux-tui/src/state/chat.rs`
- Trigger: Any assistant turn that has both reasoning and tool calls.
- Workaround: Expand the reasoning block manually to see content.

**Tool call rendering inconsistency between live streaming and history:**
- Symptoms: Live streaming shows `active_tool_calls` from `ChatState`, history shows tool messages differently. The same call looks different in real-time vs. after it completes.
- Files: `crates/amux-tui/src/state/chat.rs`, `crates/amux-tui/src/widgets/chat.rs`
- Trigger: Any completed agent turn with tool calls viewed in history.
- Workaround: None.

**TUI PTY title not parsed from OSC sequences:**
- Symptoms: Terminal pane titles always return `None`.
- Files: `crates/amux-daemon/src/pty_session.rs:302-304`
- Trigger: Any shell with title-setting escape sequences (e.g., zsh with RPROMPT).
- Workaround: None at present; title is hardcoded from session spawn parameters.

---

## Security Considerations

**API keys serialized to `config.json` in plaintext:**
- Risk: `AgentConfig` and `GatewayConfig` include `api_key`, `slack_token`, `telegram_token`, `discord_token`, `whatsapp_token`, `honcho_api_key`. The `persist_config()` call at `crates/amux-daemon/src/agent/persistence.rs:382-387` serializes the full `AgentConfig` struct (which derives `Serialize` with no `skip` directives on key fields) to `~/.tamux/agent/config.json`.
- Files: `crates/amux-daemon/src/agent/types.rs:951-1010`, `crates/amux-daemon/src/agent/persistence.rs:382-387`
- Current mitigation: The config file is user-owned and not world-readable by default on Linux. The `redact_config_value()` function in `crates/amux-daemon/src/agent/config.rs:23-40` exists for the logging/GetConfig path but is NOT applied on write.
- Recommendations: Mark sensitive fields with `#[serde(skip_serializing)]` on the `AgentConfig` struct, store API keys separately in a keyring or dedicated secret store, never write raw keys to the JSON config file.

**Gateway tokens passed as environment variables to child process:**
- Risk: In `crates/amux-daemon/src/agent/gateway_loop.rs:243-251`, Slack, Telegram, and Discord tokens are passed as environment variables to the spawned `tamux-gateway` subprocess. Environment variables are readable by any process with ptrace access on Linux, and are visible in `/proc/{pid}/environ` until the child clears them.
- Files: `crates/amux-daemon/src/agent/gateway_loop.rs:238-261`
- Current mitigation: Process runs as the same user; no multi-user isolation concern on typical desktop use.
- Recommendations: Pass tokens via stdin or a temporary credential file with restricted permissions; clear environment variables in the child after startup.

**Daemon IPC socket has no authentication:**
- Risk: The Unix socket at `$XDG_RUNTIME_DIR/tamux-daemon.sock` (see `crates/amux-daemon/src/server.rs:61-64`) accepts all connections from any process that can reach the socket. There is no auth token, capability check, or credential verification.
- Files: `crates/amux-daemon/src/server.rs:61-64`, `crates/amux-daemon/src/server.rs:114-130`
- Current mitigation: The socket is placed in `XDG_RUNTIME_DIR` which is mode 0700 on well-configured systems, limiting access to the owning user.
- Recommendations: Acceptable for a single-user desktop tool; would need HMAC challenge or capability tokens before exposing to multi-user or network-accessible environments.

**`scrub_sensitive` hex pattern will over-redact git commit SHAs and checksums:**
- Risk: The pattern `\b[0-9a-f]{40,}\b` in `crates/amux-daemon/src/scrub.rs:34` matches any 40+ character lowercase hex string. Git commit hashes (40 chars) are valid matches. This causes false-positive redaction in terminal output or tool results containing git log output.
- Files: `crates/amux-daemon/src/scrub.rs:34`
- Current mitigation: Only applied to specific contexts (tool results, memory writes).
- Recommendations: Scope hex redaction to contexts where a 40+ char hex would not be a git SHA (e.g., check for surrounding `commit ` or `tree ` prefixes before redacting).

---

## Performance Bottlenecks

**SQLite: new connection opened per operation, no WAL mode:**
- Problem: Every `HistoryStore` method calls `self.open_connection()` which opens a fresh SQLite connection. With no connection pooling and default DELETE journal mode, concurrent read/write operations from the agent loop and session manager serialize.
- Files: `crates/amux-daemon/src/history.rs:2451-2455`
- Cause: SQLite `Connection` is not `Send`/`Clone` without connection pool. Simple but high-frequency operations (command log, message appends) pay connection setup cost on every call.
- Improvement path: Wrap in `r2d2-sqlite` or `deadpool-sqlite` for pooling, and set `PRAGMA journal_mode=WAL` on first connection to allow concurrent readers with one writer.

**Broadcast channel size 256 for PTY output — easily lagged under burst:**
- Problem: `crates/amux-daemon/src/pty_session.rs:130` creates a `broadcast::channel(256)`. Agent engine uses `broadcast::channel(256)` at `crates/amux-daemon/src/agent/engine.rs:116`. Under rapid tool execution or high-throughput terminal output, slow subscribers fall behind. The server at `crates/amux-daemon/src/server.rs:271` logs `"agent event broadcast lagged"` when this happens, but events are simply dropped.
- Files: `crates/amux-daemon/src/pty_session.rs:130`, `crates/amux-daemon/src/agent/engine.rs:116`
- Cause: `tokio::sync::broadcast` drops oldest messages when the ring buffer is full for lagged receivers. A slow TUI client receiving terminal output will miss PTY data.
- Improvement path: Increase channel capacity for high-throughput sessions, or switch to a fan-out with per-subscriber buffering (e.g., `tokio::sync::mpsc` per subscriber).

**`find_symbol` regex fallback walks entire workspace on every call:**
- Problem: `crates/amux-daemon/src/validation.rs:49-94` — the LSP fallback for symbol search uses `WalkDir::new(workspace_root)` and reads every `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.md` file on each invocation. No caching, no incremental indexing.
- Files: `crates/amux-daemon/src/validation.rs:49`
- Cause: LSP client fallback is designed for correctness, not performance; no in-memory symbol index is maintained.
- Improvement path: Cache the last walk result with an mtime/inotify invalidation strategy; or prefer the LSP path and only fall back for small repos.

**Slack polling in Electron opens N `conversations.history` requests per interval:**
- Problem: `frontend/electron/main.cjs:102-158` — `pollSlackInbox()` calls `conversations.list` and then issues one `conversations.history` HTTP request per channel every 5 seconds. For workspaces with many channels, this is O(N) API calls per poll cycle.
- Files: `frontend/electron/main.cjs:102-176`
- Cause: Slack RTM/Events API (WebSocket) is not implemented; polling is the fallback. The daemon-side Slack stub is also not implemented.
- Improvement path: Switch to Slack Events API with Socket Mode to get push notifications instead of polling.

---

## Fragile Areas

**`ConciergeDetailLevel::Minimal => unreachable!()` panic in production code:**
- Files: `crates/amux-daemon/src/agent/concierge.rs:478`
- Why fragile: The `build_prompt_for_level` match arm for `Minimal` panics. If configuration or future code ever reaches this branch (e.g., a new enum variant, a serialization round-trip, or a future code path), the daemon crashes.
- Safe modification: Replace `unreachable!()` with a fallback to `template_fallback()` or an `Err` return.
- Test coverage: No test covers the `Minimal` variant path.

**`projection.rs` uses `panic!()` in the `other => panic!(...)` arms:**
- Files: `crates/amux-tui/src/projection.rs:265`, `crates/amux-tui/src/projection.rs:302`, `crates/amux-tui/src/projection.rs:324`
- Why fragile: These panics fire inside TUI event projection. If the daemon sends an unexpected event shape (e.g., after a protocol update), the TUI process crashes immediately.
- Safe modification: Return `AppAction::Status("unexpected event")` instead of panicking; remove the `#![allow(dead_code)]` file-level attribute and enable full warnings to catch dead branches.
- Test coverage: Tests in the same file cover happy paths only; no test exercises the panic branches.

**`AgentEngine::new()` calls `HistoryStore::new().expect(...)` — panics at startup:**
- Files: `crates/amux-daemon/src/agent/engine.rs:143`, `crates/amux-daemon/src/session_manager.rs:37`
- Why fragile: If `~/.tamux/` is not writable (permissions issue, full disk, NFS mount), the daemon panics during startup instead of returning an error with context.
- Safe modification: Propagate the error up through `AgentEngine::new()` returning `Result<Arc<Self>>`.
- Test coverage: No test covers startup failure paths.

**`snapshot.rs` backend init calls `.expect()` after user selects forced backend:**
- Files: `crates/amux-daemon/src/snapshot.rs:586`, `595`, `601`, `607`, `611`, `617`
- Why fragile: `detect_snapshot_backend()` is called with user-supplied preference (e.g., `"zfs"` or `"btrfs"`). If the backend constructor fails (e.g., missing `btrfs-progs`), it panics instead of returning a recoverable error.
- Safe modification: Return `Result<Box<dyn SnapshotBackend>>` from `detect_snapshot_backend`, propagate to callers.
- Test coverage: None for backend initialization failure.

**`electron/main.cjs` is a 4,500-line monolithic file:**
- Files: `frontend/electron/main.cjs`
- Why fragile: Mixes IPC registration, Discord client management, Slack polling, Telegram polling, WhatsApp bridge, PTY bridge, agent bridge, OpenAI Codex OAuth flow, window management, and file system access in a single file. Side effects (global mutable state: `discordClient`, `slackBotToken`, `telegramPollTimer`, etc.) are not encapsulated.
- Safe modification: Extract each integration (Slack, Telegram, Discord, WhatsApp, agent bridge) into dedicated modules before adding features. Any new feature added here risks unintended interaction with existing globals.
- Test coverage: Zero automated tests for Electron main process logic.

**`(window as any).tamux ?? (window as any).amux` appears in 39 source files:**
- Files: 39 frontend files (run `grep -rl "window as any.*tamux"` to list)
- Why fragile: The bridge access pattern bypasses TypeScript's type system. `amux-bridge.d.ts` declares global types, but they are opt-in and the cast to `any` means callers have no compile-time safety. A bridge method rename or removal is invisible until runtime.
- Safe modification: Export a typed `getBridge(): AmuxBridge` helper from a single module (e.g., `frontend/src/lib/bridge.ts`) and replace all inline `(window as any).tamux ?? (window as any).amux` calls with it.
- Test coverage: None for bridge availability.

---

## Scaling Limits

**Snapshot index tracks disk files but enforce_retention only removes index entries:**
- Current capacity: `enforce_retention()` in `crates/amux-daemon/src/snapshot.rs:755-808` removes entries from the SQLite index and calls `std::fs::remove_file`. However, files created before the index existed (or by a previous version that didn't index) are not removed. `cleanup_orphaned_files()` exists but is not called on a schedule.
- Limit: Disk usage grows unbounded for files orphaned from the index (e.g., from daemon crashes during snapshot create).
- Scaling path: Call `cleanup_orphaned_files()` as part of the periodic retention check, not just on demand.

**In-memory agent state grows without eviction:**
- Current capacity: `AgentEngine` holds all threads (`RwLock<HashMap<String, AgentThread>>`), all tasks, all goal runs, all heartbeat items, and all sub-agent runtimes in memory.
- Limit: Long-running daemon instances with many conversations accumulate unbounded in-memory state. `persist_thread_by_id()` writes to SQLite but does not evict from the HashMap.
- Scaling path: Add an LRU eviction policy on `self.threads` and `self.tasks`, keeping only the N most recently active threads in memory.

---

## Dependencies at Risk

**`amux-gateway` crate includes stub implementations with no functional code:**
- Risk: The crate builds successfully and registers gateway providers (Discord, Slack, Telegram) that all silently do nothing. Code that starts the gateway binary gets `warnings` but no errors. This creates a false sense of gateway functionality.
- Impact: Users who configure the daemon gateway path (not Electron path) see connected status but receive/send nothing.
- Migration plan: Either complete the implementation using `reqwest` + existing HTTP client infrastructure already used in `llm_client.rs`, or gate the binary behind a feature flag and document clearly that daemon-side gateway is not functional.

---

## Missing Critical Features

**No stream cancellation surface in TUI:**
- Problem: Users cannot cancel an in-progress LLM response from the TUI without a specific double-Esc sequence (added recently). There is no visual indicator while streaming is active, and the first Esc only shows "Press Esc again to stop" — the message disappears on the next frame if misread.
- Files: `crates/amux-tui/src/app/keyboard.rs:182-196`
- Blocks: Basic agent interaction safety.

**No snapshot visibility in TUI or frontend settings:**
- Problem: From memory note `snapshot_management.md`, the frontend shows "snapshots: 0" and the TUI has no snapshot panel. Users cannot see existing snapshots, their sizes, or manually trigger cleanup.
- Files: `crates/amux-daemon/src/snapshot.rs` (retention logic exists), `crates/amux-tui/src/widgets/settings.rs` (no snapshot section)
- Blocks: Users cannot audit or reclaim disk space without CLI access.

**TUI has no mouse click focus for panes:**
- Problem: From memory note `tui_feedback_2026-03-19.md`, clicking a pane does not focus it. `handle_mouse()` in `crates/amux-tui/src/app/mouse.rs` handles scrolling and drag-selection but does not route `MouseEventKind::Down` to set pane focus.
- Files: `crates/amux-tui/src/app/mouse.rs`
- Blocks: Intuitive multi-pane workflow; users must use keyboard Tab to cycle focus.

---

## Test Coverage Gaps

**Electron main process has zero tests:**
- What's not tested: Discord client lifecycle, Slack polling deduplication, Telegram update offset tracking, WhatsApp bridge JSON-RPC, PTY session bridging, OpenAI Codex OAuth flow, daemon health check.
- Files: `frontend/electron/main.cjs` (4,506 lines)
- Risk: Regressions in gateway integrations are invisible until users report them.
- Priority: High — the file is large, stateful, and actively modified.

**Agent integration paths are covered by unit tests only:**
- What's not tested: End-to-end `send_message` → LLM stream → tool execution → result → next turn. All existing tests are unit tests over isolated functions (compaction, memory, skill variants, config parsing).
- Files: `crates/amux-daemon/src/agent/agent_loop.rs`, `crates/amux-daemon/src/agent/tool_executor.rs`
- Risk: Regressions in the multi-turn reasoning loop go undetected until manual testing.
- Priority: High.

**TUI rendering is not tested:**
- What's not tested: Widget layout, scroll clamping, message rendering, tool call display, reasoning block ordering.
- Files: `crates/amux-tui/src/widgets/chat.rs`, `crates/amux-tui/src/widgets/settings.rs`
- Risk: Chat rendering bugs (overscroll, clipped bottom line, reasoning order) are caught only by human testing.
- Priority: Medium — ratatui supports headless buffer testing via `Buffer::with_lines`.

**No integration test for SQLite concurrency:**
- What's not tested: Concurrent writes from `SessionManager` and `AgentEngine` using separate `HistoryStore` instances.
- Files: `crates/amux-daemon/src/history.rs`
- Risk: SQLite locking errors under load are silent (operations fail and log, but callers may proceed as if successful).
- Priority: Medium.

---

*Concerns audit: 2026-03-22*
