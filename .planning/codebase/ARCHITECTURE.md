# Architecture

**Analysis Date:** 2026-03-22

## Pattern Overview

**Overall:** Daemon-first, client-multiplexed agent runtime with event-driven IPC

**Key Characteristics:**
- The daemon (`tamux-daemon`) is the single source of truth for all state: sessions, threads, tasks, goal runs, memory, and telemetry
- Multiple heterogeneous clients (Electron GUI, TUI, CLI, MCP server, chat gateway) connect to the daemon over a Unix domain socket using a shared binary protocol (`amux-protocol`)
- All agent execution — LLM inference, tool calls, sub-agent spawning — happens inside the daemon process, not in the UI
- Long-running work (goal runs, background tasks) survives UI disconnects and can be reattached by any client
- The frontend has its own Zustand state layer (stores) that mirrors and extends daemon state via IPC

## Layers

**Protocol Layer:**
- Purpose: Defines the wire format and message types shared by all crates
- Location: `crates/amux-protocol/src/messages.rs`
- Contains: `ClientMessage` enum (client → daemon), `DaemonMessage` enum (daemon → client), all supporting structs (`SessionInfo`, `ApprovalPayload`, `AgentDbThread`, etc.)
- Depends on: serde, uuid
- Used by: every other crate

**Daemon Core:**
- Purpose: Runs the Unix socket server, manages PTY sessions, owns all persistent state
- Location: `crates/amux-daemon/src/`
- Contains: `server.rs` (IPC listener), `session_manager.rs` (PTY registry), `history.rs` (SQLite store + WORM ledgers), `policy.rs`, `validation.rs`, `sandbox.rs`, `snapshot.rs`, `pty_session.rs`, `osc.rs`
- Depends on: amux-protocol, tokio, rusqlite
- Used by: nothing (top-level binary)

**Agent Engine (inside daemon):**
- Purpose: LLM inference loop, tool execution, task queue, goal runner, memory, learning, liveness
- Location: `crates/amux-daemon/src/agent/`
- Contains: `engine.rs` (AgentEngine struct), `agent_loop.rs` (hot path), `tool_executor.rs`, `task_scheduler.rs`, `goal_planner.rs`, `persistence.rs`, `memory.rs`, `concierge.rs`, subagent management (`subagent/`), learning (`learning/`), metacognitive (`metacognitive/`), liveness (`liveness/`), context (`context/`)
- Depends on: SessionManager, HistoryStore, reqwest, amux-protocol
- Used by: `server.rs` which constructs an `AgentEngine` and passes messages to it

**CLI Client:**
- Purpose: Scriptable terminal client for interacting with the daemon
- Location: `crates/amux-cli/src/`
- Contains: `main.rs` (clap-based CLI), `client.rs` (Unix socket IPC), `plugins.rs` (npm plugin install)
- Depends on: amux-protocol
- Used by: terminal users; also spawned by Electron as a bridge subprocess (`agent-bridge`, `db-bridge`, `bridge` hidden subcommands)

**TUI Client:**
- Purpose: Interactive terminal-native control plane with full agent/task/goal run visibility
- Location: `crates/amux-tui/src/`
- Contains: `main.rs` (Ratatui setup + main loop), `app/` (TuiModel, event handlers, keyboard, settings), `state/` (chat, config, auth, settings, modal, task, subagents), `widgets/` (chat, sidebar, settings, task_tree, reasoning, etc.), `client.rs` (daemon IPC), `wire.rs` (data types bridging daemon ↔ TUI)
- Depends on: amux-protocol, ratatui, crossterm
- Used by: terminal users

**Gateway Sidecar:**
- Purpose: Standalone process bridging Slack/Telegram/Discord into the daemon agent
- Location: `crates/amux-gateway/src/`
- Contains: `main.rs`, `router.rs`, `slack.rs`, `telegram.rs`, `discord.rs`
- Depends on: amux-protocol (connects to daemon as a client)
- Note: The daemon also contains a built-in gateway (`crates/amux-daemon/src/agent/gateway.rs`) that runs inline without a separate process

**MCP Server:**
- Purpose: Exposes daemon capabilities as MCP tools over JSON-RPC stdio transport
- Location: `crates/amux-mcp/src/main.rs`
- Depends on: amux-protocol
- Used by: Claude Code and other MCP-compatible agents

**Electron Main Process:**
- Purpose: Desktop app host, daemon lifecycle management, IPC bridge from renderer
- Location: `frontend/electron/main.cjs`
- Contains: Daemon process spawning, agent bridge subprocess management, WhatsApp/Discord/Slack/Telegram integrations, OpenAI Codex OAuth flow, terminal bridge multiplexing, session-level IPC handlers
- Depends on: Node.js, Electron, discord.js

**Electron Preload:**
- Purpose: Secure contextBridge exposing IPC APIs to renderer
- Location: `frontend/electron/preload.cjs`
- Contains: Plugin injection, contextBridge API definitions exposing `window.amux`

**React Frontend (renderer):**
- Purpose: Main UI — workspace/surface layout, terminal panes, agent chat panel, settings
- Location: `frontend/src/`
- Entry: `frontend/src/main.tsx` → `App.tsx` or `CDUIApp.tsx` (CDUI mode)
- Depends on: React, Zustand, Vite, xterm.js

## Data Flow

**Terminal Session Lifecycle:**

1. User/agent requests a session via `ClientMessage::SpawnSession`
2. Daemon's `SessionManager` creates a `PtySession` (PTY + OS process)
3. PTY output bytes are broadcast via a tokio `broadcast::Sender<DaemonMessage>`
4. Server dispatches `DaemonMessage::Output` to all attached clients
5. Clients render output (terminal emulator in frontend, direct write in CLI attach)

**Agent Chat Turn:**

1. Client sends `ClientMessage::AgentSendMessage { thread_id, content, ... }`
2. Server forwards to `AgentEngine::send_message_inner()`
3. Engine: get/create thread, persist user message, sync to Honcho (optional)
4. Engine: build prompt from system prompt + SOUL.md + MEMORY.md + USER.md + skill index + operator model summary + OneContext recall
5. Engine: call LLM via `llm_client::send_completion_request()` with streaming
6. Streaming deltas broadcast as `AgentEvent::Delta` via `event_tx` broadcast channel
7. Tool calls parsed → `execute_tool()` → tool result appended to thread
8. Loop continues until no more tool calls or max iterations
9. Final assistant message persisted; learning/provenance/liveness updated
10. `AgentEvent::Done` broadcast to subscribers

**Goal Run Lifecycle:**

1. Client sends `ClientMessage::AgentStartGoalRun { goal, ... }`
2. AgentEngine creates `GoalRun`, calls planning model to produce step plan
3. Steps converted to `AgentTask` entries in the task queue
4. `task_scheduler` picks up tasks, dispatches each through `send_message_inner()`
5. Goal run monitors task outcomes, replans on failure up to `max_replans`
6. On completion: reflection, memory update, optional skill generation

**State Management:**

- Daemon state: in-process `RwLock`/`Mutex`-guarded data structures, persisted to SQLite via `HistoryStore` at write time
- Frontend state: Zustand stores (`agentStore`, `workspaceStore`, `agentMissionStore`, etc.), hydrated from `window.localStorage`/Electron IPC on startup, synced to daemon via IPC calls

## Key Abstractions

**AgentEngine:**
- Purpose: The entire in-daemon agent system — threads, tasks, goal runs, memory, tools, learning, liveness, gateways
- Examples: `crates/amux-daemon/src/agent/engine.rs`, `crates/amux-daemon/src/agent/mod.rs`
- Pattern: Large `Arc`-wrapped struct with `RwLock`/`Mutex` on each field. Behavior in extension modules (`agent_loop.rs`, `task_scheduler.rs`, etc.) via `impl AgentEngine` blocks

**SessionManager:**
- Purpose: Registry of all running PTY sessions; handles spawn/attach/kill/managed commands/approvals
- Examples: `crates/amux-daemon/src/session_manager.rs`
- Pattern: `Arc<Self>` shared between server and agent engine; sessions stored as `HashMap<SessionId, Arc<Mutex<PtySession>>>`

**HistoryStore:**
- Purpose: SQLite persistence layer for threads, messages, command log, transcripts, snapshots, goal runs, WORM telemetry ledgers
- Examples: `crates/amux-daemon/src/history.rs`
- Pattern: Wraps a SQLite path, opens connections per operation using rusqlite

**ClientMessage / DaemonMessage:**
- Purpose: Complete protocol surface between any client and the daemon
- Examples: `crates/amux-protocol/src/messages.rs`
- Pattern: Tagged enums serialized with bincode via `AmuxCodec` (tokio-util `LengthDelimitedCodec` + bincode)

**Zustand Stores:**
- Purpose: Frontend reactive state with persistence hydration
- Examples: `frontend/src/lib/workspaceStore.ts`, `frontend/src/lib/agentStore.ts`, `frontend/src/lib/agentMissionStore.ts`, `frontend/src/lib/settingsStore.ts`
- Pattern: `create()` from zustand, each store has a `hydrate*()` function called during bootstrap; mutations dispatched imperatively or via IPC callbacks

**CommandRegistry / ComponentRegistry:**
- Purpose: Extensibility points — commands and components can be registered by name and invoked/rendered dynamically
- Examples: `frontend/src/registry/commandRegistry.ts`, `frontend/src/registry/componentRegistry.ts`
- Pattern: Plain `Map<string, fn>` singletons; used by CDUI mode for dynamic UI

## Entry Points

**Daemon:**
- Location: `crates/amux-daemon/src/main.rs`
- Triggers: Direct binary invocation (`tamux-daemon`) or spawned by Electron/CLI
- Responsibilities: Logging setup, state restore, start `server::run()` (blocks)

**CLI:**
- Location: `crates/amux-cli/src/main.rs`
- Triggers: `tamux <subcommand>` from terminal or Electron subprocess
- Responsibilities: Parse clap args, connect to daemon socket, dispatch request

**TUI:**
- Location: `crates/amux-tui/src/main.rs`
- Triggers: Direct binary invocation
- Responsibilities: Enter alternate screen, create `TuiModel`, run 50ms tick loop with daemon bridge thread

**Electron:**
- Location: `frontend/electron/main.cjs`
- Triggers: `electron .` or packaged app launch
- Responsibilities: Create `BrowserWindow`, spawn `tamux-daemon` if not running, register IPC handlers for terminal/agent/db bridges, manage sidecar processes

**React (renderer):**
- Location: `frontend/src/main.tsx`
- Triggers: Electron loads `index.html`, which loads `main.tsx`
- Responsibilities: Hydrate all Zustand stores, restore persisted session, render `App` or `CDUIApp`

**MCP Server:**
- Location: `crates/amux-mcp/src/main.rs`
- Triggers: MCP client invocation (e.g. Claude Code)
- Responsibilities: Read JSON-RPC requests from stdin, connect to daemon socket, dispatch and return results

## Error Handling

**Strategy:** `anyhow::Result<T>` propagation in Rust; `DaemonMessage::Error { message }` and `AgentError { message }` for client-visible failures; uncaught errors in JS are caught by `ViewErrorBoundary`

**Patterns:**
- Daemon operations use `?` propagation with `anyhow::Context` for error context
- IPC server catches errors per-connection and sends `DaemonMessage::Error` back to client before closing
- Frontend catches IPC errors in callbacks and updates store state to show error messages
- Agent engine uses a circuit breaker (`circuit_breaker.rs`) and liveness/recovery system (`liveness/`) to detect and recover from stuck agents

## Cross-Cutting Concerns

**Logging:** `tracing` crate with `tracing-subscriber` + `tracing-appender`. Daemon writes to `~/.tamux/tamux-daemon.log`, CLI to `tamux-cli.log`, TUI to `tamux-tui.log`. Log level controlled by `TAMUX_LOG` env var.

**Validation:** Commands validated before execution in `crates/amux-daemon/src/validation.rs`; security policy enforced in `policy.rs` and `policy_external.rs`

**Authentication:** LLM provider API keys stored in daemon `config.json`; provider auth states exposed via `AgentGetProviderAuthStates`; OpenAI Codex uses OAuth flow in Electron (`main.cjs`)

**Persistence:** SQLite via `HistoryStore` for structured data; markdown files (`SOUL.md`, `MEMORY.md`, `USER.md`) for agent identity/memory; JSON files in `~/.tamux/` for config, gateway thread maps, session snapshots

---

*Architecture analysis: 2026-03-22*
