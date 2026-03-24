# Technology Stack

**Analysis Date:** 2026-03-22

## Languages

**Primary:**
- Rust (stable channel, edition 2021) - All backend crates: daemon, CLI, TUI, protocol, gateway, MCP server
- TypeScript 5.6 - Electron frontend, React UI, all frontend lib/component code
- JavaScript (CommonJS) - Electron main process (`frontend/electron/main.cjs`, `preload.cjs`, `whatsapp-bridge.cjs`)

**Secondary:**
- Shell (bash/ps1) - Build and release scripts in `scripts/`
- YAML - Agent skills and config files (parsed via `serde_yaml` in daemon)

## Runtime

**Rust Environment:**
- Toolchain: stable (pinned via `rust-toolchain.toml`)
- Components: `rustfmt`, `clippy`
- Target: native (Linux, Windows, macOS)

**Node / Electron:**
- Node.js (managed by Electron)
- Electron 33.x (`frontend/package.json` devDependencies)
- Package Manager: npm
- Lockfile: `frontend/package-lock.json` (present)

## Frameworks

**Core (Rust):**
- Tokio 1.x (`features = ["full"]`) - Async runtime for all Rust crates
- Serde 1.x + serde_json 1.x - Serialization throughout
- `tokio-util` 0.7 with codec - Framed binary IPC over Unix socket / TCP / named pipe

**Frontend UI:**
- React 19.x - UI component framework
- Zustand 5.x - Frontend state management (all stores in `frontend/src/lib/`)
- Vite 6.x - Build tool and dev server (config: `frontend/vite.config.ts`)
- xterm.js (`@xterm/xterm` 5.5.x) with addons: canvas, fit, search, serialize, web-links, WebGL - Terminal emulator
- React Flow (`@xyflow/react` 12.x) - Execution canvas / agent graph visualization
- `react-resizable-panels` 2.x - Resizable panel layout
- `react-markdown` 10.x + `remark-gfm` - Markdown rendering in chat

**TUI (Rust):**
- Ratatui 0.29 with crossterm backend - Terminal UI framework
- `ratatui-textarea` 0.8 - Text input widgets
- `tui-markdown` 0.3 - Markdown rendering in TUI
- `arboard` 3.x - Clipboard access from TUI
- `ureq` 3.x - Synchronous HTTP for TUI auth flows

**Testing:**
- Rust built-in `#[test]` framework - Unit tests inline in daemon/protocol crates
- No external test framework detected in frontend

**Build/Dev:**
- `electron-builder` 25.x - Packages Electron app for Windows (NSIS, portable), Linux (AppImage, deb), macOS (dmg, zip)
- TypeScript 5.6 with strict mode - Frontend type checking (`frontend/tsconfig.json`)
- ESLint - Frontend linting (`"lint": "eslint ."` script)
- `@vitejs/plugin-react` 4.3 - Vite React plugin

## Key Dependencies

**Critical (Rust):**
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

**Critical (Frontend):**
- `@honcho-ai/sdk` 2.x - Cross-session memory provider integration
- `discord.js` 14.x - Discord bot client for gateway messaging (in Electron main)
- `@whiskeysockets/baileys` 6.7 - WhatsApp Web multi-device bridge (in `electron/whatsapp-bridge.cjs`)
- `zod` 4.x - Schema validation (frontend config/message types)
- `js-yaml` 4.x - YAML parsing (agent skills, CDUI views)
- `pino` 9.x - Structured logging in frontend

**Infrastructure (shared):**
- `amux-protocol` (internal crate `tamux-protocol`) - Binary-framed IPC message types shared across all crates
- `bincode` 1.x - Binary serialization for IPC frames
- `bytes` 1.x - Byte buffer management for IPC codec
- `futures` 0.3 + `tokio-stream` 0.1 - Async streaming primitives
- `uuid` 1.x (`features = ["v4", "serde"]`) - Session/thread/message IDs
- `humantime` 2.x - Human-readable time formatting in logs
- `tracing` 0.1 + `tracing-subscriber` 0.3 + `tracing-appender` 0.2 - Structured async logging
- `anyhow` 1.x + `thiserror` 2.x - Error handling

## Configuration

**Daemon Configuration:**
- Agent config: `~/.tamux/agent/config.json` (JSON, loaded at startup, live-reloaded on file change)
- Key fields: `provider`, `model`, `api_key`, `base_url`, `api_transport`, `gateway`, `tools`, `honcho_*`
- Sensitive keys are redacted in logs: `api_key`, `slack_token`, `telegram_token`, `discord_token`, `whatsapp_token`, `firecrawl_api_key`, `exa_api_key`, `tavily_api_key`, `honcho_api_key`
- Data directory: `~/.tamux/` (history SQLite, memory markdown files, tasks JSON, skills)

**Frontend Configuration:**
- All settings persisted via `window.tamux` / `window.amux` Electron bridge to app data directory
- Settings stored as JSON files via `readPersistedJson` / `scheduleJsonWrite` in `frontend/src/lib/persistence.ts`
- Path aliases: `@/*` maps to `frontend/src/*` (TypeScript and Vite)
- TypeScript strict mode enabled (`noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`)

**Build:**
- Electron app build config: `frontend/package.json` `"build"` section
- Rust build: standard `cargo build --release`
- Combined release script: `scripts/build-release.sh` (Linux/macOS/WSL), `scripts/build-release.bat`/`.ps1` (Windows)
- Output binaries bundled into Electron app as `extraResources`: `tamux-daemon`, `tamux`

## Platform Requirements

**Development:**
- Rust stable toolchain
- Node.js (LTS recommended, Electron 33.x compatible)
- npm (lockfile present)
- Optional: `aline` CLI on PATH (OneContext search feature)
- Optional: `typescript-language-server`, `rust-analyzer`, `pylsp` on PATH (LSP symbol search)
- Optional: `hermes`, `openclaw` CLIs (alternative agent backends)

**Production / Deployment:**
- Desktop app: Electron self-contained executable (Electron embeds Node + Chromium)
- Daemon: standalone native binary `tamux-daemon` (bundled inside Electron release)
- CLI: standalone native binary `tamux` (bundled inside Electron release)
- TUI: standalone native binary `tamux-tui` (separate, not bundled in Electron release)
- No server-side hosting — fully local desktop application

---

*Stack analysis: 2026-03-22*
