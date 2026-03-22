# Codebase Structure

**Analysis Date:** 2026-03-22

## Directory Layout

```
cmux-next/
├── Cargo.toml              # Workspace root — 6 member crates
├── Cargo.lock
├── rust-toolchain.toml     # Pinned Rust toolchain
├── AGENTS.md               # Agent onboarding instructions
├── README.md
├── crates/
│   ├── amux-daemon/        # Core daemon binary (tamux-daemon)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs
│   │       ├── session_manager.rs
│   │       ├── history.rs
│   │       ├── pty_session.rs
│   │       ├── policy.rs
│   │       ├── policy_external.rs
│   │       ├── validation.rs
│   │       ├── sandbox.rs
│   │       ├── snapshot.rs
│   │       ├── state.rs
│   │       ├── osc.rs
│   │       ├── scrub.rs
│   │       ├── git.rs
│   │       ├── criu.rs
│   │       ├── lsp_client.rs
│   │       ├── network.rs
│   │       └── agent/      # Agent engine (entire AI subsystem)
│   │           ├── mod.rs
│   │           ├── engine.rs
│   │           ├── engine_runtime.rs
│   │           ├── agent_loop.rs
│   │           ├── tool_executor.rs
│   │           ├── llm_client.rs
│   │           ├── types.rs
│   │           ├── config.rs
│   │           ├── persistence.rs
│   │           ├── task_scheduler.rs
│   │           ├── task_crud.rs
│   │           ├── task_prompt.rs
│   │           ├── thread_crud.rs
│   │           ├── goal_planner.rs
│   │           ├── goal_llm.rs
│   │           ├── goal_parsing.rs
│   │           ├── concierge.rs
│   │           ├── memory.rs
│   │           ├── memory_flush.rs
│   │           ├── gateway.rs
│   │           ├── gateway_loop.rs
│   │           ├── dispatcher.rs
│   │           ├── collaboration.rs
│   │           ├── heartbeat.rs
│   │           ├── honcho.rs
│   │           ├── metadata.rs
│   │           ├── system_prompt.rs
│   │           ├── session_recall.rs
│   │           ├── semantic_env.rs
│   │           ├── skill_evolution.rs
│   │           ├── skill_preflight.rs
│   │           ├── tool_synthesis.rs
│   │           ├── work_context.rs
│   │           ├── operational_context.rs
│   │           ├── operator_model.rs
│   │           ├── anticipatory.rs
│   │           ├── behavioral_events.rs
│   │           ├── causal_traces.rs
│   │           ├── circuit_breaker.rs
│   │           ├── compaction.rs
│   │           ├── provenance.rs
│   │           ├── rate_limiter.rs
│   │           ├── external_runner.rs
│   │           ├── external_messaging.rs
│   │           ├── messaging.rs
│   │           ├── context/    # Context management subsystem
│   │           │   ├── mod.rs
│   │           │   ├── context_item.rs
│   │           │   ├── compression.rs
│   │           │   ├── archive.rs
│   │           │   ├── audit.rs
│   │           │   └── restoration.rs
│   │           ├── subagent/   # Sub-agent spawning and supervision
│   │           │   ├── mod.rs
│   │           │   ├── lifecycle.rs
│   │           │   ├── supervisor.rs
│   │           │   ├── termination.rs
│   │           │   ├── tool_filter.rs
│   │           │   ├── tool_graph.rs
│   │           │   └── context_budget.rs
│   │           ├── learning/   # Pattern learning and effectiveness
│   │           │   ├── mod.rs
│   │           │   ├── effectiveness.rs
│   │           │   ├── heuristics.rs
│   │           │   ├── patterns.rs
│   │           │   └── traces.rs
│   │           ├── liveness/   # Agent health monitoring and recovery
│   │           │   ├── mod.rs
│   │           │   ├── checkpoint.rs
│   │           │   ├── health_monitor.rs
│   │           │   ├── recovery.rs
│   │           │   ├── state_layers.rs
│   │           │   └── stuck_detection.rs
│   │           └── metacognitive/  # Self-assessment and replanning
│   │               ├── mod.rs
│   │               ├── escalation.rs
│   │               ├── replanning.rs
│   │               ├── resource_alloc.rs
│   │               └── self_assessment.rs
│   ├── amux-protocol/      # Shared IPC types (tamux-protocol crate)
│   │   └── src/
│   │       └── messages.rs
│   ├── amux-cli/           # CLI binary (tamux)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── client.rs
│   │       └── plugins.rs
│   ├── amux-tui/           # TUI binary (tamux-tui)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── app/        # TuiModel + event handlers
│   │       ├── state/      # TUI reactive state slices
│   │       ├── widgets/    # Ratatui widget implementations
│   │       ├── client.rs
│   │       ├── wire.rs
│   │       ├── projection.rs
│   │       ├── providers.rs
│   │       ├── auth.rs
│   │       └── theme.rs
│   ├── amux-gateway/       # Chat gateway sidecar (tamux-gateway)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── router.rs
│   │       ├── slack.rs
│   │       ├── telegram.rs
│   │       └── discord.rs
│   └── amux-mcp/           # MCP server (tamux-mcp)
│       └── src/
│           └── main.rs
├── frontend/               # Electron + React desktop app
│   ├── electron/
│   │   ├── main.cjs        # Electron main process
│   │   ├── preload.cjs     # Context bridge (secure IPC)
│   │   └── whatsapp-bridge.cjs
│   ├── src/
│   │   ├── main.tsx        # React entry point + store hydration
│   │   ├── App.tsx         # Main application shell
│   │   ├── CDUIApp.tsx     # Dynamic/CDUI mode app shell
│   │   ├── components/     # React UI components
│   │   ├── lib/            # Zustand stores + utility functions
│   │   ├── registry/       # Command and component registries
│   │   ├── plugins/        # Plugin system (ai-training, coding-agents)
│   │   ├── renderers/      # Dynamic renderer for CDUI views
│   │   ├── schemas/        # UI schema types
│   │   ├── context/        # React context providers
│   │   ├── hooks/          # Custom React hooks
│   │   ├── types/          # TypeScript declaration files
│   │   └── styles/
│   ├── package.json
│   ├── vite.config.ts
│   └── tsconfig.json
├── docs/                   # Architecture docs
├── scripts/                # Build/release scripts
├── dist-release/           # Pre-built release binaries
├── .planning/              # GSD planning documents
│   └── codebase/
├── .claude/                # Claude agent config
├── .codex/                 # Codex agent config
└── todo/                   # Project todos
```

## Directory Purposes

**`crates/amux-daemon/src/`:**
- Purpose: The running daemon process — all state ownership, session management, agent runtime
- Key files: `server.rs` (IPC server entry), `session_manager.rs` (PTY registry), `history.rs` (SQLite)
- Key sub-directory: `agent/` — the entire AI/agent subsystem lives here

**`crates/amux-daemon/src/agent/`:**
- Purpose: Self-contained agent engine module — everything from LLM calls to sub-agent supervision
- Key files: `engine.rs` (struct def), `agent_loop.rs` (hot path), `tool_executor.rs` (tools), `types.rs` (all agent types), `persistence.rs` (hydrate/persist), `task_scheduler.rs` (task queue), `goal_planner.rs` (multi-step goal runs)
- Pattern: Behavior is split across many `impl AgentEngine` files; `mod.rs` re-exports all and uses `use super::*` to share types across siblings

**`crates/amux-protocol/src/`:**
- Purpose: The single shared crate for the wire protocol — imported by all other crates
- Key file: `messages.rs` — the only source file; defines all `ClientMessage`/`DaemonMessage` variants and supporting types

**`crates/amux-tui/src/`:**
- Purpose: Ratatui terminal UI — parallel feature set to the Electron frontend
- Key sub-directories: `app/` (model + handlers), `state/` (reactive state slices), `widgets/` (rendering)
- Key files: `main.rs` (event loop), `wire.rs` (types bridging IPC ↔ TUI), `client.rs` (daemon IPC)

**`frontend/electron/`:**
- Purpose: Electron main process + preload. Manages daemon process, terminal bridges, OAuth, sidecar integrations
- Key files: `main.cjs` (all IPC handlers, bridge management), `preload.cjs` (contextBridge)

**`frontend/src/lib/`:**
- Purpose: All Zustand stores and non-component business logic
- Key stores: `workspaceStore.ts` (layout/surfaces/panes), `agentStore.ts` (provider config, threads, messages), `agentMissionStore.ts` (events, approvals, snapshots), `settingsStore.ts`, `commandLogStore.ts`, `snippetStore.ts`
- Utilities: `agentClient.ts` (LLM API), `agentTools.ts` (tool definitions for frontend agent), `persistence.ts` (localStorage helpers), `bspTree.ts` (binary space partitioning for layout)

**`frontend/src/components/`:**
- Purpose: All React components — flat list of top-level panels + sub-directories for complex panels
- Pattern: Complex panels decomposed into `<PanelName>/` subdirectory with subcomponents and a `shared.ts`/`shared.tsx` for types

**`frontend/src/registry/`:**
- Purpose: Dynamic command and component extensibility registries used by CDUI mode and plugins
- Key files: `commandRegistry.ts`, `componentRegistry.ts`, `registerBaseCommands.ts`, `registerBaseComponents.ts`

**`frontend/src/plugins/`:**
- Purpose: Plugin system — ai-training and coding-agents plugins bundled in-repo
- Pattern: Each plugin has `registerPlugin.ts`, `store.ts`, `types.ts`, `bridge.ts`, `definitions.ts`

**`frontend/src/renderers/`:**
- Purpose: Dynamic CDUI view rendering
- Key files: `DynamicRenderer.tsx` (renders views by type), `ViewErrorBoundary.tsx`

## Key File Locations

**Entry Points:**
- `crates/amux-daemon/src/main.rs`: Daemon binary entry
- `crates/amux-cli/src/main.rs`: CLI binary entry
- `crates/amux-tui/src/main.rs`: TUI binary entry
- `crates/amux-gateway/src/main.rs`: Gateway sidecar entry
- `crates/amux-mcp/src/main.rs`: MCP server entry
- `frontend/electron/main.cjs`: Electron main process entry
- `frontend/src/main.tsx`: React renderer entry

**Configuration:**
- `Cargo.toml`: Workspace dependencies and member crates
- `rust-toolchain.toml`: Pinned Rust toolchain version
- `frontend/package.json`: Frontend dependencies
- `frontend/vite.config.ts`: Vite build config
- `frontend/tsconfig.json`: TypeScript config

**Core Logic:**
- `crates/amux-protocol/src/messages.rs`: All IPC message types
- `crates/amux-daemon/src/agent/engine.rs`: AgentEngine struct definition
- `crates/amux-daemon/src/agent/agent_loop.rs`: LLM turn execution
- `crates/amux-daemon/src/agent/tool_executor.rs`: Tool dispatch
- `crates/amux-daemon/src/agent/types.rs`: Agent type definitions (AgentConfig, AgentTask, GoalRun, etc.)
- `crates/amux-daemon/src/server.rs`: IPC server loop
- `crates/amux-daemon/src/session_manager.rs`: PTY session registry
- `crates/amux-daemon/src/history.rs`: SQLite persistence layer
- `frontend/src/lib/agentStore.ts`: Frontend agent state, provider definitions
- `frontend/src/lib/workspaceStore.ts`: Layout/workspace state
- `frontend/src/lib/agentClient.ts`: LLM API calls from frontend

**Testing:**
- `crates/amux-daemon/src/agent/mod.rs`: Contains the only substantial test module (`#[cfg(test)] mod tests`) with unit tests for task queue logic, goal run projections, and planner detection

## Naming Conventions

**Rust Files:**
- Modules named in `snake_case.rs` matching their purpose (e.g. `agent_loop.rs`, `tool_executor.rs`)
- Sub-systems organized as directories with `mod.rs` (e.g. `agent/subagent/mod.rs`)
- All Rust crates share the `amux-` prefix (package names use `tamux-` prefix in `Cargo.toml`)

**TypeScript Files:**
- Stores named `<domain>Store.ts` (e.g. `agentStore.ts`, `workspaceStore.ts`)
- Components named `PascalCase.tsx` (e.g. `AgentChatPanel.tsx`, `SettingsPanel.tsx`)
- Complex panels decomposed under `kebab-case/` subdirectories
- Shared types within panel subdirectories in `shared.ts` or `shared.tsx`
- Custom hooks prefixed `use` (e.g. `useHotkeys.ts`, `useTerminalClipboard.ts`)

**Directories:**
- Rust: `snake_case` subdirectories for module groups
- Frontend components: `kebab-case` for panel-level subdirectory decomposition (e.g. `agent-chat-panel/`, `settings-panel/`)

## Where to Add New Code

**New agent tool (daemon-side):**
- Add tool definition in `crates/amux-daemon/src/agent/tool_executor.rs` (`get_available_tools()`)
- Add execution handler in the same file (`execute_tool()` match arm)
- Add any new config flags to `AgentConfig` in `crates/amux-daemon/src/agent/types.rs`

**New `ClientMessage` request:**
- Add variant to `ClientMessage` enum in `crates/amux-protocol/src/messages.rs`
- Add matching `DaemonMessage` response variant
- Handle in `crates/amux-daemon/src/server.rs` dispatch function
- Add `AgentEngine` method if agent-related

**New frontend panel/modal:**
- Create `frontend/src/components/MyPanel.tsx`
- If complex, create `frontend/src/components/my-panel/` directory with `shared.ts` for types
- Add lazy import in `frontend/src/App.tsx`
- Add toggle state to `useWorkspaceStore` in `frontend/src/lib/workspaceStore.ts` if needed

**New Zustand store:**
- Create `frontend/src/lib/<domain>Store.ts`
- Export `use<Domain>Store` hook and `hydrate<Domain>Store()` function
- Call `hydrate<Domain>Store()` in `frontend/src/main.tsx` bootstrap sequence

**New TUI widget:**
- Create `crates/amux-tui/src/widgets/<name>.rs`
- Register in `crates/amux-tui/src/widgets/mod.rs`
- Add corresponding state slice in `crates/amux-tui/src/state/` if needed

**New daemon subsystem (non-agent):**
- Create `crates/amux-daemon/src/<name>.rs`
- Declare as `mod <name>;` in `crates/amux-daemon/src/main.rs`

**New agent sub-system:**
- Create `crates/amux-daemon/src/agent/<name>.rs`
- Declare in `crates/amux-daemon/src/agent/mod.rs`
- Use `impl AgentEngine` in the new file and import via `use super::*;`

## Special Directories

**`.planning/codebase/`:**
- Purpose: GSD analysis documents (this file and peers)
- Generated: No (written by GSD mapper agents)
- Committed: Yes

**`.claude/`:**
- Purpose: Claude agent memory and project instructions
- Generated: No
- Committed: Yes

**`.codex/`:**
- Purpose: OpenAI Codex agent config
- Generated: No
- Committed: Yes

**`.history/`:**
- Purpose: Git-tracked history of previous file states (VS Code Local History extension)
- Generated: Yes
- Committed: Yes

**`frontend/dist/`:**
- Purpose: Vite build output
- Generated: Yes
- Committed: No (in .gitignore)

**`target/`:**
- Purpose: Cargo build artifacts
- Generated: Yes
- Committed: No (in .gitignore)

**`dist-release/`:**
- Purpose: Pre-built release binaries checked into repo for distribution
- Generated: Yes (by build scripts)
- Committed: Yes

**`docs/`:**
- Purpose: Architecture and onboarding documentation
- Key files: `docs/how-tamux-works.md`, `docs/self-orchestrating-agent.md`
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-03-22*
