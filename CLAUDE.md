<!-- GSD:project-start source:PROJECT.md -->
## Project

**tamux — The Agent That Lives**

tamux is a daemon-first, self-orchestrating AI agent runtime that lives on your machine, remembers everything it learns, ships work while you sleep, and gets smarter every day. It's a local desktop application (Electron + TUI + CLI) powered by a Rust daemon that owns all state — threads, tasks, goal runs, memory, telemetry, and terminal sessions. Multiple clients (desktop GUI, terminal UI, CLI, MCP server, chat gateways) connect to the same daemon, so long-running work survives UI disconnects and can be reattached from any surface.

tamux is not a chatbot wrapper. It is the most architecturally deep open-source agent runtime in existence — 4-layer self-orchestration, genetic skill evolution, sub-agent management, crash-recoverable goal runs, WORM audit trails, and an operator model that learns how you work. The next milestone is about making that depth *felt* — turning infrastructure into experience.

**Core Value:** **An agent that feels alive and gets smarter over time — while remaining simple enough that anyone can understand what it's doing and why.**

Depth without clarity is wasted engineering. Every capability must surface as a simple, understandable experience. If the user can't feel it or explain it, it doesn't count.

### Constraints

- **Tech stack**: Rust daemon + TypeScript/React frontend — no language changes, Rust performance and safety are core to the daemon-first architecture
- **Local-first**: All data stays on the operator's machine — no phone-home, no cloud dependency, no account required
- **Provider-agnostic**: Must work with any OpenAI-compatible or Anthropic-compatible LLM provider — no vendor lock-in
- **Backward compatibility**: Existing `~/.tamux/` data directory, config format, and IPC protocol must not break for current users
- **Single binary aspiration**: Reducing install friction is critical — aiming for single-binary or near-single-binary distribution
- **Platform parity**: Linux, macOS, Windows all first-class (Electron handles GUI; Rust handles daemon/CLI/TUI)
<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->
## Technology Stack

## Languages
- Rust (stable channel, edition 2021) - All backend crates: daemon, CLI, TUI, protocol, gateway, MCP server
- TypeScript 5.6 - Electron frontend, React UI, all frontend lib/component code
- JavaScript (CommonJS) - Electron main process (`frontend/electron/main.cjs`, `preload.cjs`, `whatsapp-bridge.cjs`)
- Shell (bash/ps1) - Build and release scripts in `scripts/`
- YAML - Agent skills and config files (parsed via `serde_yaml` in daemon)
## Runtime
- Toolchain: stable (pinned via `rust-toolchain.toml`)
- Components: `rustfmt`, `clippy`
- Target: native (Linux, Windows, macOS)
- Node.js (managed by Electron)
- Electron 33.x (`frontend/package.json` devDependencies)
- Package Manager: npm
- Lockfile: `frontend/package-lock.json` (present)
## Frameworks
- Tokio 1.x (`features = ["full"]`) - Async runtime for all Rust crates
- Serde 1.x + serde_json 1.x - Serialization throughout
- `tokio-util` 0.7 with codec - Framed binary IPC over Unix socket / TCP / named pipe
- React 19.x - UI component framework
- Zustand 5.x - Frontend state management (all stores in `frontend/src/lib/`)
- Vite 6.x - Build tool and dev server (config: `frontend/vite.config.ts`)
- xterm.js (`@xterm/xterm` 5.5.x) with addons: canvas, fit, search, serialize, web-links, WebGL - Terminal emulator
- React Flow (`@xyflow/react` 12.x) - Execution canvas / agent graph visualization
- `react-resizable-panels` 2.x - Resizable panel layout
- `react-markdown` 10.x + `remark-gfm` - Markdown rendering in chat
- Ratatui 0.29 with crossterm backend - Terminal UI framework
- `ratatui-textarea` 0.8 - Text input widgets
- `tui-markdown` 0.3 - Markdown rendering in TUI
- `arboard` 3.x - Clipboard access from TUI
- `ureq` 3.x - Synchronous HTTP for TUI auth flows
- Rust built-in `#[test]` framework - Unit tests inline in daemon/protocol crates
- No external test framework detected in frontend
- `electron-builder` 25.x - Packages Electron app for Windows (NSIS, portable), Linux (AppImage, deb), macOS (dmg, zip)
- TypeScript 5.6 with strict mode - Frontend type checking (`frontend/tsconfig.json`)
- ESLint - Frontend linting (`"lint": "eslint ."` script)
- `@vitejs/plugin-react` 4.3 - Vite React plugin
## Key Dependencies
- `rusqlite` 0.32 (`features = ["bundled"]`) - Embedded SQLite for daemon history/sessions/memory. Bundled — no external SQLite required.
- `portable-pty` 0.8 - Cross-platform PTY (pseudo-terminal) for terminal session management
- `reqwest` 0.12 (`features = ["json", "stream"]`) - HTTP client for LLM API calls and webhook requests (SSE streaming)
- `tree-sitter` 0.22 + `tree-sitter-bash` 0.21 - Code parsing for symbol search and bash command analysis
- `notify` 6.x - Filesystem watching (agent config live reload)
- `interprocess` 2.x (Windows only, `features = ["tokio"]`) - Named pipe IPC on Windows
- `clap` 4.x (`features = ["derive"]`) - CLI argument parsing for `tamux` CLI
- `sha2` 0.10 - Cryptographic hashing for WORM ledger integrity and session snapshots
- `base64` 0.22 - Encoding for vision screenshots and binary data transfer
- `jsonrepair` 0.1 - Repair malformed LLM JSON tool call responses
- `serde_yaml` 0.9 - YAML parsing for agent skills and config
- `walkdir` 2.x - Directory traversal for file tools
- `which` 7.x - PATH binary detection (LSP servers, `aline` CLI)
- `regex` 1.x - ANSI escape stripping and pattern matching
- `sysinfo` 0.30 - System info for daemon health
- `strip-ansi-escapes` 0.2 - Clean terminal output for agent context
- `@honcho-ai/sdk` 2.x - Cross-session memory provider integration
- `discord.js` 14.x - Discord bot client for gateway messaging (in Electron main)
- `@whiskeysockets/baileys` 6.7 - WhatsApp Web multi-device bridge (in `electron/whatsapp-bridge.cjs`)
- `zod` 4.x - Schema validation (frontend config/message types)
- `js-yaml` 4.x - YAML parsing (agent skills, CDUI views)
- `pino` 9.x - Structured logging in frontend
- `amux-protocol` (internal crate `tamux-protocol`) - Binary-framed IPC message types shared across all crates
- `bincode` 1.x - Binary serialization for IPC frames
- `bytes` 1.x - Byte buffer management for IPC codec
- `futures` 0.3 + `tokio-stream` 0.1 - Async streaming primitives
- `uuid` 1.x (`features = ["v4", "serde"]`) - Session/thread/message IDs
- `humantime` 2.x - Human-readable time formatting in logs
- `tracing` 0.1 + `tracing-subscriber` 0.3 + `tracing-appender` 0.2 - Structured async logging
- `anyhow` 1.x + `thiserror` 2.x - Error handling
## Configuration
- Agent config: `~/.tamux/agent/config.json` (JSON, loaded at startup, live-reloaded on file change)
- Key fields: `provider`, `model`, `api_key`, `base_url`, `api_transport`, `gateway`, `tools`, `honcho_*`
- Sensitive keys are redacted in logs: `api_key`, `slack_token`, `telegram_token`, `discord_token`, `whatsapp_token`, `firecrawl_api_key`, `exa_api_key`, `tavily_api_key`, `honcho_api_key`
- Data directory: `~/.tamux/` (history SQLite, memory markdown files, tasks JSON, skills)
- All settings persisted via `window.tamux` / `window.amux` Electron bridge to app data directory
- Settings stored as JSON files via `readPersistedJson` / `scheduleJsonWrite` in `frontend/src/lib/persistence.ts`
- Path aliases: `@/*` maps to `frontend/src/*` (TypeScript and Vite)
- TypeScript strict mode enabled (`noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`)
- Electron app build config: `frontend/package.json` `"build"` section
- Rust build: standard `cargo build --release`
- Combined release script: `scripts/build-release.sh` (Linux/macOS/WSL), `scripts/build-release.bat`/`.ps1` (Windows)
- Output binaries bundled into Electron app as `extraResources`: `tamux-daemon`, `tamux`
## Platform Requirements
- Rust stable toolchain
- Node.js (LTS recommended, Electron 33.x compatible)
- npm (lockfile present)
- Optional: `aline` CLI on PATH (OneContext search feature)
- Optional: `typescript-language-server`, `rust-analyzer`, `pylsp` on PATH (LSP symbol search)
- Optional: `hermes`, `openclaw` CLIs (alternative agent backends)
- Desktop app: Electron self-contained executable (Electron embeds Node + Chromium)
- Daemon: standalone native binary `tamux-daemon` (bundled inside Electron release)
- CLI: standalone native binary `tamux` (bundled inside Electron release)
- TUI: standalone native binary `tamux-tui` (separate, not bundled in Electron release)
- No server-side hosting — fully local desktop application
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

## Rust Conventions
### Naming Patterns
- `snake_case` throughout — e.g., `circuit_breaker.rs`, `tool_filter.rs`, `session_recall.rs`
- Submodule directories use the same convention: `crates/amux-daemon/src/agent/metacognitive/`, `crates/amux-daemon/src/agent/subagent/`
- `PascalCase` — e.g., `CircuitBreaker`, `CircuitState`, `TokenBucket`, `MemoryTarget`, `AgentThread`
- `snake_case` — e.g., `record_failure`, `try_acquire`, `build_causal_guidance_summary`
- Boolean query methods use the predicate pattern: `is_allowed`, `has_restrictions`, `can_execute`
- Builder helpers in tests use `make_` prefix: `make_tool`, `make_item`, `make_msg`, `make_input`
- Factory helpers use `sample_` prefix: `sample_goal_run`, `sample_task`, `sample_provider_config`
- `UPPER_SNAKE_CASE` — e.g., `SOUL_LIMIT_CHARS`, `ONECONTEXT_TOOL_OUTPUT_MAX_CHARS`, `APPROX_CHARS_PER_TOKEN`
- Constants with numeric suffixes: `CONCIERGE_THREAD_ID`, `MIN_CONTEXT_TARGET_TOKENS`
- Internal module helpers: `pub(super)` — e.g., in `crates/amux-daemon/src/agent/memory.rs`
- Cross-crate internals: `pub(crate)`
- Public API: `pub`
- Default (private): no modifier; all internal implementation details
### Module Documentation
### Type Definitions
- Use `#[derive(Debug, Clone)]` as the minimum for domain types
- Types meant to be copied cheaply add `Copy, PartialEq, Eq`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- Wire types (protocol messages) use: `#[derive(Debug, Clone, Serialize, Deserialize)]`
- Serde enums always carry `#[serde(rename_all = "snake_case")]` unless field-level overrides are needed
- Optional wire fields use: `#[serde(default, skip_serializing_if = "Option::is_none")]`
### Error Handling
- `anyhow` is the primary error crate across all Rust crates (declared in `Cargo.toml`)
- `thiserror` is in workspace deps but reserved for typed errors; most errors use `anyhow::anyhow!(...)`
### Import Organization
### Code Organization
### Default Implementations
## TypeScript / React Conventions
### Naming Patterns
- React components: `PascalCase.tsx` — e.g., `ChatView.tsx`, `AgentTab.tsx`, `TitleBar.tsx`
- Stores and lib utilities: `camelCase.ts` — e.g., `agentStore.ts`, `workspaceStore.ts`, `agentClient.ts`
- Component subdirectories: `kebab-case/` — e.g., `agent-chat-panel/`, `settings-panel/`, `base-components/`
- Named exports only (no default exports from component files, except the root `App`)
- `function ComponentName(...)` syntax (not arrow functions for top-level components)
- Props passed as inline destructured object parameter with explicit TypeScript type annotation
- `PascalCase` for all type names: `AgentThread`, `ChatChunk`, `ProviderDefinition`
- `type` for unions and aliases, `interface` for object shapes — both are used interchangeably
- Provider IDs are string union types: `export type AgentProviderId = "featherless" | "openai" | ...`
- `camelCase` — e.g., `createWorkspace`, `toggleSidebar`, `normalizeAgentProviderId`
- Constants: `UPPER_SNAKE_CASE` — e.g., `AGENT_PROVIDER_IDS`, `PROVIDER_DEFINITIONS`, `APPROX_CHARS_PER_TOKEN`
- Counter helpers: prefixed with `_` when private module-level: `let _wsId = 0`
- Store hook: `use<Name>Store` — e.g., `useAgentStore`, `useWorkspaceStore`, `useSettingsStore`
- Selector pattern: `const thing = useStore((s) => s.thing)` — granular subscriptions, not full store access
- Actions defined inline within `create(...)` call
### TypeScript Settings
- `"strict": true`
- `"noUnusedLocals": true`
- `"noUnusedParameters": true`
- `"noFallthroughCasesInSwitch": true`
- `"forceConsistentCasingInFileNames": true`
### Import Organization
### Component Design
### Error Handling
### Logging
### Comments
## Shared Across Both Languages
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

## Pattern Overview
- The daemon (`tamux-daemon`) is the single source of truth for all state: sessions, threads, tasks, goal runs, memory, and telemetry
- Multiple heterogeneous clients (Electron GUI, TUI, CLI, MCP server, chat gateway) connect to the daemon over a Unix domain socket using a shared binary protocol (`amux-protocol`)
- All agent execution — LLM inference, tool calls, sub-agent spawning — happens inside the daemon process, not in the UI
- Long-running work (goal runs, background tasks) survives UI disconnects and can be reattached by any client
- The frontend has its own Zustand state layer (stores) that mirrors and extends daemon state via IPC
## Layers
- Purpose: Defines the wire format and message types shared by all crates
- Location: `crates/amux-protocol/src/messages.rs`
- Contains: `ClientMessage` enum (client → daemon), `DaemonMessage` enum (daemon → client), all supporting structs (`SessionInfo`, `ApprovalPayload`, `AgentDbThread`, etc.)
- Depends on: serde, uuid
- Used by: every other crate
- Purpose: Runs the Unix socket server, manages PTY sessions, owns all persistent state
- Location: `crates/amux-daemon/src/`
- Contains: `server.rs` (IPC listener), `session_manager.rs` (PTY registry), `history.rs` (SQLite store + WORM ledgers), `policy.rs`, `validation.rs`, `sandbox.rs`, `snapshot.rs`, `pty_session.rs`, `osc.rs`
- Depends on: amux-protocol, tokio, rusqlite
- Used by: nothing (top-level binary)
- Purpose: LLM inference loop, tool execution, task queue, goal runner, memory, learning, liveness
- Location: `crates/amux-daemon/src/agent/`
- Contains: `engine.rs` (AgentEngine struct), `agent_loop.rs` (hot path), `tool_executor.rs`, `task_scheduler.rs`, `goal_planner.rs`, `persistence.rs`, `memory.rs`, `concierge.rs`, subagent management (`subagent/`), learning (`learning/`), metacognitive (`metacognitive/`), liveness (`liveness/`), context (`context/`)
- Depends on: SessionManager, HistoryStore, reqwest, amux-protocol
- Used by: `server.rs` which constructs an `AgentEngine` and passes messages to it
- Purpose: Scriptable terminal client for interacting with the daemon
- Location: `crates/amux-cli/src/`
- Contains: `main.rs` (clap-based CLI), `client.rs` (Unix socket IPC), `plugins.rs` (npm plugin install)
- Depends on: amux-protocol
- Used by: terminal users; also spawned by Electron as a bridge subprocess (`agent-bridge`, `db-bridge`, `bridge` hidden subcommands)
- Purpose: Interactive terminal-native control plane with full agent/task/goal run visibility
- Location: `crates/amux-tui/src/`
- Contains: `main.rs` (Ratatui setup + main loop), `app/` (TuiModel, event handlers, keyboard, settings), `state/` (chat, config, auth, settings, modal, task, subagents), `widgets/` (chat, sidebar, settings, task_tree, reasoning, etc.), `client.rs` (daemon IPC), `wire.rs` (data types bridging daemon ↔ TUI)
- Depends on: amux-protocol, ratatui, crossterm
- Used by: terminal users
- Purpose: Standalone process bridging Slack/Telegram/Discord into the daemon agent
- Location: `crates/amux-gateway/src/`
- Contains: `main.rs`, `router.rs`, `slack.rs`, `telegram.rs`, `discord.rs`
- Depends on: amux-protocol (connects to daemon as a client)
- Note: The daemon also contains a built-in gateway (`crates/amux-daemon/src/agent/gateway.rs`) that runs inline without a separate process
- Purpose: Exposes daemon capabilities as MCP tools over JSON-RPC stdio transport
- Location: `crates/amux-mcp/src/main.rs`
- Depends on: amux-protocol
- Used by: Claude Code and other MCP-compatible agents
- Purpose: Desktop app host, daemon lifecycle management, IPC bridge from renderer
- Location: `frontend/electron/main.cjs`
- Contains: Daemon process spawning, agent bridge subprocess management, WhatsApp/Discord/Slack/Telegram integrations, OpenAI Codex OAuth flow, terminal bridge multiplexing, session-level IPC handlers
- Depends on: Node.js, Electron, discord.js
- Purpose: Secure contextBridge exposing IPC APIs to renderer
- Location: `frontend/electron/preload.cjs`
- Contains: Plugin injection, contextBridge API definitions exposing `window.amux`
- Purpose: Main UI — workspace/surface layout, terminal panes, agent chat panel, settings
- Location: `frontend/src/`
- Entry: `frontend/src/main.tsx` → `App.tsx` or `CDUIApp.tsx` (CDUI mode)
- Depends on: React, Zustand, Vite, xterm.js
## Data Flow
- Daemon state: in-process `RwLock`/`Mutex`-guarded data structures, persisted to SQLite via `HistoryStore` at write time
- Frontend state: Zustand stores (`agentStore`, `workspaceStore`, `agentMissionStore`, etc.), hydrated from `window.localStorage`/Electron IPC on startup, synced to daemon via IPC calls
## Key Abstractions
- Purpose: The entire in-daemon agent system — threads, tasks, goal runs, memory, tools, learning, liveness, gateways
- Examples: `crates/amux-daemon/src/agent/engine.rs`, `crates/amux-daemon/src/agent/mod.rs`
- Pattern: Large `Arc`-wrapped struct with `RwLock`/`Mutex` on each field. Behavior in extension modules (`agent_loop.rs`, `task_scheduler.rs`, etc.) via `impl AgentEngine` blocks
- Purpose: Registry of all running PTY sessions; handles spawn/attach/kill/managed commands/approvals
- Examples: `crates/amux-daemon/src/session_manager.rs`
- Pattern: `Arc<Self>` shared between server and agent engine; sessions stored as `HashMap<SessionId, Arc<Mutex<PtySession>>>`
- Purpose: SQLite persistence layer for threads, messages, command log, transcripts, snapshots, goal runs, WORM telemetry ledgers
- Examples: `crates/amux-daemon/src/history.rs`
- Pattern: Wraps a SQLite path, opens connections per operation using rusqlite
- Purpose: Complete protocol surface between any client and the daemon
- Examples: `crates/amux-protocol/src/messages.rs`
- Pattern: Tagged enums serialized with bincode via `AmuxCodec` (tokio-util `LengthDelimitedCodec` + bincode)
- Purpose: Frontend reactive state with persistence hydration
- Examples: `frontend/src/lib/workspaceStore.ts`, `frontend/src/lib/agentStore.ts`, `frontend/src/lib/agentMissionStore.ts`, `frontend/src/lib/settingsStore.ts`
- Pattern: `create()` from zustand, each store has a `hydrate*()` function called during bootstrap; mutations dispatched imperatively or via IPC callbacks
- Purpose: Extensibility points — commands and components can be registered by name and invoked/rendered dynamically
- Examples: `frontend/src/registry/commandRegistry.ts`, `frontend/src/registry/componentRegistry.ts`
- Pattern: Plain `Map<string, fn>` singletons; used by CDUI mode for dynamic UI
## Entry Points
- Location: `crates/amux-daemon/src/main.rs`
- Triggers: Direct binary invocation (`tamux-daemon`) or spawned by Electron/CLI
- Responsibilities: Logging setup, state restore, start `server::run()` (blocks)
- Location: `crates/amux-cli/src/main.rs`
- Triggers: `tamux <subcommand>` from terminal or Electron subprocess
- Responsibilities: Parse clap args, connect to daemon socket, dispatch request
- Location: `crates/amux-tui/src/main.rs`
- Triggers: Direct binary invocation
- Responsibilities: Enter alternate screen, create `TuiModel`, run 50ms tick loop with daemon bridge thread
- Location: `frontend/electron/main.cjs`
- Triggers: `electron .` or packaged app launch
- Responsibilities: Create `BrowserWindow`, spawn `tamux-daemon` if not running, register IPC handlers for terminal/agent/db bridges, manage sidecar processes
- Location: `frontend/src/main.tsx`
- Triggers: Electron loads `index.html`, which loads `main.tsx`
- Responsibilities: Hydrate all Zustand stores, restore persisted session, render `App` or `CDUIApp`
- Location: `crates/amux-mcp/src/main.rs`
- Triggers: MCP client invocation (e.g. Claude Code)
- Responsibilities: Read JSON-RPC requests from stdin, connect to daemon socket, dispatch and return results
## Error Handling
- Daemon operations use `?` propagation with `anyhow::Context` for error context
- IPC server catches errors per-connection and sends `DaemonMessage::Error` back to client before closing
- Frontend catches IPC errors in callbacks and updates store state to show error messages
- Agent engine uses a circuit breaker (`circuit_breaker.rs`) and liveness/recovery system (`liveness/`) to detect and recover from stuck agents
## Cross-Cutting Concerns
<!-- GSD:architecture-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
