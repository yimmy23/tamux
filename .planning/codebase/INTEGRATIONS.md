# External Integrations

**Analysis Date:** 2026-03-22

## APIs & External Services

### LLM Providers

All LLM calls are made directly from the client (frontend via `fetch()`, or daemon via `reqwest`). Two API wire formats are supported:
- **OpenAI-compatible** (`/chat/completions` with Bearer auth) — default for most providers
- **Anthropic Messages API** (`/v1/messages` with `x-api-key`) — used for Anthropic-type providers

Provider registry is defined in:
- Frontend: `frontend/src/lib/agentStore.ts` (`PROVIDER_DEFINITIONS` array)
- Daemon: `crates/amux-daemon/src/agent/types.rs` (`PROVIDER_DEFINITIONS` static array)

**Supported Providers:**

| Provider ID | Name | API Format | Default Base URL |
|---|---|---|---|
| `openai` | OpenAI / ChatGPT | OpenAI (responses + chat) | `https://api.openai.com/v1` |
| `featherless` | Featherless | OpenAI (chat) | `https://api.featherless.ai/v1` |
| `qwen` | Qwen (Alibaba) | OpenAI native assistant | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` |
| `qwen-deepinfra` | Qwen (DeepInfra) | OpenAI (chat) | `https://api.deepinfra.com/v1/openai` |
| `kimi` | Kimi (Moonshot) | OpenAI (chat) | `https://api.moonshot.ai/v1` |
| `kimi-coding-plan` | Kimi Coding Plan | OpenAI (chat) | `https://api.kimi.com/coding/v1` |
| `z.ai` | Z.AI (GLM) | OpenAI (chat) | `https://api.z.ai/api/paas/v4` |
| `z.ai-coding-plan` | Z.AI Coding Plan | OpenAI (chat) | `https://api.z.ai/api/coding/paas/v4` |
| `openrouter` | OpenRouter | OpenAI (chat) | `https://openrouter.ai/api/v1` |
| `cerebras` | Cerebras | OpenAI (chat) | `https://api.cerebras.ai/v1` |
| `together` | Together | OpenAI (chat) | `https://api.together.xyz/v1` |
| `groq` | Groq | OpenAI (responses + chat) | `https://api.groq.com/openai/v1` |
| `ollama` | Ollama | OpenAI (chat) | `http://localhost:11434/v1` |
| `chutes` | Chutes | OpenAI (chat) | `https://llm.chutes.ai/v1` |
| `huggingface` | Hugging Face | OpenAI (chat) | `https://api-inference.huggingface.co/v1` |
| `minimax` | MiniMax | Anthropic | `https://api.minimax.io/anthropic` |
| `minimax-coding-plan` | MiniMax Coding Plan | Anthropic | `https://api.minimax.io/anthropic` |
| `alibaba-coding-plan` | Alibaba Coding Plan | Anthropic | `https://coding-intl.dashscope.aliyuncs.com/apps/anthropic` |
| `opencode-zen` | OpenCode Zen | Anthropic | `https://opencode.ai/zen/v1` |
| `custom` | Custom | OpenAI (configurable) | User-defined |

**Auth:**
- Most providers: API key in `AgentConfig.api_key` (config.json field)
- OpenAI: also supports `chatgpt_subscription` auth source (OAuth via Codex auth flow)
- Codex OAuth: PKCE flow managed in Electron main, stores tokens in `~/.codex/auth.json` and `tamux-data/openai-codex-auth.json`

**Transport Modes:**
- `chat_completions` — `/chat/completions` POST with SSE streaming
- `responses` — OpenAI Responses API (supports `previousResponseId` for continuity)
- `native_assistant` — Alibaba DashScope Assistant API (Qwen provider only)

**Client Implementations:**
- Frontend: `frontend/src/lib/agentClient.ts` — direct `fetch()` calls
- Daemon: `crates/amux-daemon/src/agent/llm_client.rs` — `reqwest` with SSE streaming

---

### Honcho AI Memory

- **What:** Cross-session persistent memory and context retrieval
- **SDK:** `@honcho-ai/sdk` v2.x (frontend, lazy-loaded in `frontend/src/lib/honchoClient.ts`)
- **Daemon client:** `crates/amux-daemon/src/agent/honcho.rs` (raw HTTP via `reqwest`)
- **Default base URL:** `https://api.honcho.dev`
- **Auth:** `honcho_api_key` field in agent config
- **Workspace:** `honcho_workspace_id` (defaults to `"tamux"`)
- **Activation:** opt-in via `enable_honcho_memory: true` in config
- **Frontend tool:** `queryHonchoMemory()` / `buildHonchoContext()` / `syncMessagesToHoncho()` in `frontend/src/lib/honchoClient.ts`
- **Daemon tool:** exposed as `agent_query_memory` tool in `crates/amux-daemon/src/agent/tool_executor.rs`

---

### Aline (OneContext) Search

- **What:** Conversational history search across Claude Code sessions
- **Integration:** External CLI binary (`aline`) invoked as subprocess
- **Daemon usage:** `crates/amux-daemon/src/agent/tool_executor.rs` `execute_onecontext_search()` — calls `aline search <query> -t <scope>`
- **Frontend usage:** `frontend/src/lib/agentTools.ts` (frontend-side tool invocation)
- **Availability check:** `which::which("aline")` cached via `OnceLock` in `crates/amux-daemon/src/agent/engine.rs`
- **Scopes:** `session`, `event`, `turn`
- **Timeout:** 8 seconds

---

### DuckDuckGo Web Search

- **What:** Zero-config fallback web search (no API key required)
- **Endpoint:** `https://lite.duckduckgo.com/lite/?q=<query>&kl=us-en`
- **Client:** `reqwest` in daemon, HTML-parsed response
- **Daemon tool:** `web_search` in `crates/amux-daemon/src/agent/tool_executor.rs`
- **Activation:** `config.tools.web_search = true`

---

### Language Servers (LSP)

- **What:** Workspace symbol search via JSON-RPC stdio
- **Client:** `crates/amux-daemon/src/lsp_client.rs`
- **Auto-detected servers:** `typescript-language-server`, `rust-analyzer`, `pylsp`, `jdtls`, `clangd` (PATH detection via `which`)
- **No external service:** spawns language server processes locally

---

## Data Storage

**Databases:**
- SQLite (embedded via `rusqlite` with `bundled` feature)
  - Path: `~/.tamux/history.db` (managed by `crates/amux-daemon/src/history.rs`)
  - Contains: command history, agent threads, messages, goal runs, snapshots, skill variants, provenance records, WORM event chain
  - Accessed: daemon process only (single-writer)

**File Storage:**
- Agent config: `~/.tamux/agent/config.json`
- Agent memory (markdown): `~/.tamux/agent/memory/` directory
- Agent tasks: `~/.tamux/agent/tasks.json`
- Goal runs: `~/.tamux/agent/goal-runs.json`
- Todos: `~/.tamux/agent/todos.json`
- Work context: `~/.tamux/agent/work-context.json`
- Skills: `~/.tamux/agent/skills/` directory
- Vision screenshots (tmp): `~/.tamux/tmp/vision/` (10-minute TTL)
- Frontend JSON state: App data directory via Electron bridge (`readPersistedJson`/`scheduleJsonWrite` in `frontend/src/lib/persistence.ts`)
- OpenAI Codex auth: `~/.codex/auth.json` (imported), `<app-data>/openai-codex-auth.json` (stored)

**Caching:**
- In-memory only: Honcho sync state (LRU set up to 10,000 message IDs), aline availability (OnceLock)

## Authentication & Identity

**LLM Provider Auth:**
- Bearer token (API key) stored in agent config field `api_key`
- Per-provider configs in `AgentConfig.providers` map
- OpenAI Codex special OAuth PKCE flow:
  - Client ID: `app_EMoamEEZ73f0CkXaXp7hrann`
  - Authorize URL: `https://auth.openai.com/oauth/authorize`
  - Token URL: `https://auth.openai.com/oauth/token`
  - Redirect: `http://localhost:1455/auth/callback`
  - Scopes: `openid profile email offline_access`
  - Managed in `frontend/electron/main.cjs` `ipcMain.handle('openai-codex-auth-login', ...)`

**No user accounts / server auth:** Fully local desktop app, no tamux-hosted auth.

## Monitoring & Observability

**Error Tracking:**
- None (no Sentry or similar)

**Logs:**
- Rust: `tracing` crate, configured via `tracing-subscriber` with `env-filter` (env var `RUST_LOG`)
- Log files: `tracing-appender` used in daemon, CLI, and gateway crates (rolling file appender)
- Frontend/Electron: `pino` structured logger
- Electron main: custom `logToFile()` function writing to app data directory

## Messaging Platform Gateways

All gateway integrations run as polling loops (no webhooks). Implemented in two places:
1. **Daemon-native** (no Electron dependency): `crates/amux-daemon/src/agent/gateway.rs` / `gateway_loop.rs`
2. **Electron-side** (requires Electron runtime): `frontend/electron/main.cjs`

### Discord
- **What:** Receive messages from Discord, send replies
- **SDK (Electron):** `discord.js` v14.x
- **Daemon:** REST API polling via `reqwest` (no SDK)
- **Auth:** Bot token (`discord_token` in `GatewayConfig`)
- **Config:** `discordChannelFilter`, `discordAllowedUsers`
- **Daemon tools:** `send_discord_message` tool in `tool_executor.rs`

### Slack
- **What:** Receive messages, send replies to channels
- **Implementation:** Direct REST API polling (`conversations.list` + `conversations.history`, `chat.postMessage`)
- **Both daemon and Electron:** `https://slack.com/api/` endpoints
- **Auth:** Bot token (`slack_token` in `GatewayConfig`)
- **Config:** `slackChannelFilter`
- **Poll interval:** 5 seconds
- **Daemon tools:** `send_slack_message` tool

### Telegram
- **What:** Receive messages via long-poll, send replies
- **Implementation:** `https://api.telegram.org/bot{token}/getUpdates` long-polling
- **Both daemon and Electron:** direct HTTP requests
- **Auth:** Bot token (`telegram_token` in `GatewayConfig`)
- **Config:** `telegramAllowedChats`
- **Daemon tools:** `send_telegram_message` tool

### WhatsApp
- **What:** Receive and send WhatsApp messages
- **Implementation:** `@whiskeysockets/baileys` (WhatsApp Web multi-device protocol)
- **Sidecar process:** `frontend/electron/whatsapp-bridge.cjs` — spawned as child process from Electron main
- **Communication:** JSON-RPC over stdio pipe between Electron main and sidecar
- **Auth:** Session stored in `whatsapp-auth/` directory; QR code scan required on first connect
- **Config:** `whatsapp_allowed_contacts`, `whatsapp_token` (phone number ID for Business API variant), `whatsapp_phone_id`

## CI/CD & Deployment

**Hosting:**
- Desktop app — no server hosting
- Distribution targets: Linux (AppImage, deb), Windows (portable .exe, NSIS installer), macOS (dmg, zip)

**CI Pipeline:**
- GitHub Funding config: `.github/FUNDING.yml`
- No CI pipeline detected (no `.github/workflows/` or `.gitlab-ci.yml`)

**Build scripts:**
- `scripts/build-release.sh` — Linux/macOS/WSL: builds Rust crates, frontend, packages with electron-builder
- `scripts/build-release.bat` / `build-release.ps1` — Windows equivalent
- `scripts/build-production-releases.sh` — Multi-platform production release
- `scripts/bump-version.sh` — Version bump across workspace

## Environment Configuration

**Required env vars (optional — most config goes in `~/.tamux/agent/config.json`):**
- `RUST_LOG` — Rust log level filter (e.g. `info`, `debug`)
- `XDG_RUNTIME_DIR` — Unix socket directory override (defaults to `/tmp`)
- `TAMUX_MCP_FRAMING` — Set to `content-length` for LSP-style framing in MCP server

**Secrets location:**
- All API keys stored in `~/.tamux/agent/config.json` (user home directory)
- OpenAI Codex OAuth tokens in `~/.codex/auth.json` and app data directory
- WhatsApp session credentials in `whatsapp-auth/` directory (managed by Baileys)
- No `.env` file — no server-side secrets management needed

## Webhooks & Callbacks

**Incoming:**
- None (all external integrations use client-side polling)

**Outgoing:**
- None (messaging sent via SDK/REST calls, not webhooks)

## IPC Transport (Internal)

The daemon communicates with all clients (CLI, TUI, Electron, MCP) over a binary-framed protocol defined in `crates/amux-protocol/`.

- **Unix (Linux/macOS):** Unix domain socket at `$XDG_RUNTIME_DIR/tamux-daemon.sock`
- **Windows:** Named pipe via `interprocess` crate
- **TCP (secondary):** TCP listener on `127.0.0.1:17563` (used by Electron main for direct agent bridge, `DAEMON_TCP_PORT` in `frontend/electron/main.cjs`)
- **Framing:** `AmuxCodec` (bincode-serialized, length-prefixed frames) via `tokio-util` codec
- **MCP server:** `crates/amux-mcp/` — JSON-RPC over stdio, connecting to daemon socket

---

*Integration audit: 2026-03-22*
