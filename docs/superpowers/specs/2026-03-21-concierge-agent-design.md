# Concierge Agent Design Spec

**Date:** 2026-03-21
**Status:** Approved
**Author:** Human + Claude

## Problem

tamux has no proactive engagement on app open. Users land in an empty chat with no context about their last session, pending work, or system state. There is no lightweight operational assistant for quick tasks that don't warrant the main (expensive) agent.

## Solution

A dedicated **ConciergeEngine** module — a first-class daemon component alongside the heartbeat and gateway systems. It owns a permanent pinned thread, fires a context-aware greeting on client connect, and serves as a lightweight operational assistant for housekeeping, lookups, status checks, and light automation.

## Requirements

- Proactive greeting on every client connect (TUI or Electron)
- Configurable detail level (4 tiers) with Smart Triage as default
- Persistent pinned thread always visible in the sidebar
- Falls back to main agent provider/model when no concierge-specific model is configured
- Auto-cleanup: welcome messages pruned when user navigates elsewhere
- Smart routing: "continue session X" navigates to that thread, "start new" creates a fresh thread
- Purpose-built ops tool set (no shell, no file ops, no coding)
- Handoff to main agent for tasks beyond concierge capability

---

## Data Model

### ConciergeConfig

Added as a field on `AgentConfig`:

```rust
pub struct ConciergeConfig {
    pub enabled: bool,                           // default: true
    pub detail_level: ConciergeDetailLevel,      // default: ProactiveTriage
    pub provider: Option<String>,                // None = use main agent's provider
    pub model: Option<String>,                   // None = use main agent's model
    pub auto_cleanup_on_navigate: bool,          // default: true
}
```

**Thread discovery:** The concierge thread uses a well-known fixed ID: `"concierge"`. On `initialize()`, query `history.list_threads()` for this ID — if not found, create it. This avoids storing runtime state in user config and survives config resets.

### ConciergeDetailLevel

```rust
pub enum ConciergeDetailLevel {
    Minimal,          // "Quick Hello" — session title, date, action prompt. No LLM call.
    ContextSummary,   // "Session Recap" — 1-2 sentence LLM summary of last session.
    ProactiveTriage,  // "Smart Triage" (default) — summary + pending tasks, alerts, unfinished goals.
    DailyBriefing,    // "Full Briefing" — everything above + system health, gateways, snapshots.
}
```

### AgentThread Extension

Add `pinned: bool` to `AgentThread`. Pinned threads sort first in the sidebar and are excluded from bulk cleanup.

**Persistence:** Store `pinned` in the existing `metadata_json: Option<String>` field on `AgentDbThread` as `{"pinned": true}`. On hydration, deserialize the metadata and set `pinned` accordingly. This avoids a SQLite schema migration.

### New AgentEvent Variant

```rust
AgentEvent::ConciergeWelcome {
    thread_id: String,
    content: String,
    detail_level: ConciergeDetailLevel,
    actions: Vec<ConciergeAction>,
}
```

**Event forwarding:** Add `ConciergeWelcome` to the broadcast-always list in `server.rs::should_forward_agent_event()` alongside `HeartbeatResult` and `Notification`. The welcome must reach clients that haven't yet subscribed to the concierge thread.

### ConciergeAction

Structured quick-action buttons rendered alongside the greeting text:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConciergeActionType {
    ContinueSession,
    StartNew,
    Search,
    Dismiss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConciergeAction {
    pub label: String,                       // "Continue: auth middleware refactor"
    pub action_type: ConciergeActionType,
    pub thread_id: Option<String>,           // for ContinueSession
}
```

---

## Module Architecture

### ConciergeEngine

File: `crates/amux-daemon/src/agent/concierge.rs`

A peer to `heartbeat.rs` and `gateway_loop.rs`. Initialized at daemon startup, owns its own lifecycle.

```rust
pub struct ConciergeEngine {
    config: Arc<RwLock<AgentConfig>>,
    history: HistoryStore,
    event_tx: broadcast::Sender<AgentEvent>,
    http_client: reqwest::Client,
    pending_welcome_ids: RwLock<Vec<String>>,
}
```

The concierge thread always uses the well-known ID `"concierge"` — no runtime state needed.

### Lifecycle

1. **Daemon starts** — `ConciergeEngine::new()` alongside `AgentEngine::new()`
2. **`concierge.initialize()`** — Load or create pinned concierge thread (well-known ID `"concierge"`)
3. **Client connects** (`AgentSubscribe` received) — `concierge.on_client_connected()`
4. **Context gathering** — Based on `detail_level`, query history/tasks/health
5. **Compose and emit** — Welcome message + actions via `AgentEvent::ConciergeWelcome`
6. **Ongoing** — Handle messages to the concierge thread via cheap model with ops tools

### LLM Calls

The concierge makes its own LLM calls via the existing `crate::agent::llm_client::stream_completion()` function — it does not go through `AgentEngine.send_message_inner()`. This avoids competing with the main agent, keeps the concierge system prompt separate, and allows a different provider/model.

To make the call, `ConciergeEngine` must resolve a `ProviderConfig` (base_url, api_key, auth_method, transport, model) from either the concierge-specific provider or the main agent config. Extract a shared helper:

```rust
fn resolve_concierge_provider(config: &AgentConfig) -> Result<ProviderConfig>
```

This checks `config.concierge.provider` first, falls back to `config.provider`, and resolves credentials via `config.providers[provider_id]` — the same logic as `resolve_provider_config` but without task-specific overrides.

Then call `llm_client::stream_completion(http_client, &provider_config, messages, tools)` directly.

For `Minimal` level, no LLM call is made — pure Rust template from query results.

### Provider Resolution

1. If `concierge.provider` and `concierge.model` are set, use those with credentials from `config.providers[provider]`
2. If not set, fall back to `config.provider` and `config.model` (main agent)
3. This means zero configuration gives working concierge on the main model; setting a cheap model optimizes cost

---

## Context Gathering by Detail Level

| Level | Data Sources | LLM? |
|-------|-------------|------|
| **Minimal** | `history.list_threads(limit=1)` | No — template: "Last session: {title} ({date}). Continue, start new, or search?" |
| **ContextSummary** | + `history.list_messages(last_thread, limit=10)` | Yes — summarize last session in 1-2 sentences |
| **ProactiveTriage** | + `engine.list_tasks()` (queued/in-progress) + recent `HeartbeatResult` events with alert outcomes from last 24h + pending approvals | Yes — summary + actionable triage |
| **DailyBriefing** | + system health status + gateway connectivity (Slack/Discord/Telegram online?) + snapshot stats (count, total size) + time since last activity | Yes — full operational briefing |

---

## Concierge Tool Set

Restricted, purpose-built tools. No shell, file ops, or coding capabilities.

| Tool | Category | Description |
|------|----------|-------------|
| `list_recent_sessions` | Lookup | Last N threads with title, date, message count |
| `search_sessions` | Lookup | Full-text search over session history |
| `get_session_summary` | Lookup | Fetch last N messages from a thread |
| `list_pending_tasks` | Status | Active/queued tasks with title, status, age |
| `list_pending_approvals` | Status | Approvals awaiting user decision |
| `get_heartbeat_status` | Status | Latest heartbeat outcomes and alerts |
| `get_system_health` | Status | Gateway connectivity, provider auth, snapshots |
| `set_reminder` | Automation | Create a heartbeat item with a one-shot schedule |
| `compact_old_threads` | Housekeeping | Archive/delete threads older than N days |
| `prune_snapshots` | Housekeeping | Trigger snapshot retention cleanup |
| `navigate_to_session` | Routing | Emit event to switch UI to a specific thread |
| `create_new_session` | Routing | Create a fresh thread and navigate to it |

### Concierge System Prompt

```
You are the tamux concierge — a lightweight operational assistant.
You handle greetings, session navigation, status checks, housekeeping,
and quick lookups. For coding tasks, deep analysis, or complex work,
tell the user to switch to the main agent thread.

Be concise. One paragraph max for greetings. Use bullet points for
status summaries. Always offer 2-3 actionable next steps.
```

---

## Routing & Cleanup

### Welcome Flow State Machine

```
                    ┌──────────────┐
  client connects → │  GATHERING   │  concierge queries history/tasks/health
                    └──────┬───────┘
                           │ context ready
                    ┌──────▼───────┐
                    │   GREETING   │  welcome message + actions emitted
                    └──────┬───────┘
                           │
              ┌────────────┼────────────────┐
              │            │                │
      user responds   user clicks       user navigates
      (free text)     action button     elsewhere
              │            │                │
        ┌─────▼────┐  ┌───▼──────────┐  ┌──▼──────────┐
        │  ACTIVE   │  │   ROUTING    │  │  CLEANUP    │
        │ concierge │  │              │  │ prune       │
        │ converses │  │ continue X → │  │ welcome msg │
        │ normally  │  │  navigate,   │  │ thread      │
        │           │  │  prune msg   │  │ stays       │
        └───────────┘  │              │  └─────────────┘
                       │ start new →  │
                       │  create,     │
                       │  navigate,   │
                       │  prune msg   │
                       │              │
                       │ search →     │
                       │  run search, │
                       │  show inline │
                       └──────────────┘
```

### Routing Rules

| User Action | Result |
|-------------|--------|
| Free-text response in concierge thread | ACTIVE mode — concierge LLM handles as ops conversation. Welcome stays as history. |
| Click "Continue: \<session\>" | Navigate to that thread. Prune welcome message. |
| Click "Start new session" | Create new thread, navigate to it. Prune welcome message. |
| Click "Search history" | ACTIVE mode — concierge runs search, shows clickable results. |
| Click "Dismiss" | Prune welcome message. Thread stays idle. |
| Open different thread from sidebar | Auto-cleanup — prune welcome message. |
| Disconnect without interaction | Stale welcome pruned on next `on_client_connected()` before generating new welcome. No disconnect hook needed. |

### Message Pruning (Not Thread Deletion)

The concierge thread is permanent. Only transient welcome messages are pruned. Track IDs in `ConciergeEngine.pending_welcome_ids`. On any cleanup trigger, remove those messages and clear the vec.

**Implementation:** Add `HistoryStore::delete_messages(thread_id: &str, message_ids: &[&str]) -> Result<()>` that deletes from both the in-memory `AgentThread.messages` vec and the SQLite `messages` table. Also remove from the thread's in-memory messages vec via `AgentEngine.threads` write lock.

### Reconnection

On connect with stale welcome from previous session:
1. Prune old welcome messages
2. Gather fresh context
3. Emit new welcome

The concierge thread never accumulates ignored greetings.

### Multi-Client Behavior

The daemon may have multiple simultaneous clients (TUI + Electron, or multiple TUI instances). The concierge treats welcome messages as **idempotent per session**: `on_client_connected()` generates one welcome batch and stores its message IDs. If a second client connects while the welcome is pending, it receives the existing `ConciergeWelcome` event (replayed from state) rather than generating a duplicate. Any client's cleanup action (navigate, dismiss) prunes for all clients since the messages are shared in the same thread.

### Main Agent Handoff

When user asks something beyond concierge capability:
1. Concierge: "That's a job for the main agent. Let me take you there."
2. Emit `navigate_to_session` for main thread (or create new)
3. Optionally prepend user's request as first message in target thread

---

## UI Surface

### TUI

- **Thread list:** Concierge thread pinned at top of the chat thread list with distinct label (e.g., `◆ Concierge`). The thread list is rendered in the chat panel's thread picker widget (`widgets/thread_picker.rs`), not the sidebar file tree (`widgets/sidebar.rs`).
- **Welcome message:** System-styled message with different accent color and `CONCIERGE` badge
- **Action buttons:** Clickable chips below the greeting text
- **Auto-cleanup:** Welcome pruned when user clicks another thread
- **Settings:** `Concierge` fields in settings — `enabled` toggle, `detail_level` via modal list selector (same pattern as provider/model), `provider` and `model` fields

### Electron Frontend

- **Toast notification** on connect with greeting text and inline action buttons
- **Click toast** navigates to concierge thread in chat panel
- **Concierge thread** pinned in sidebar, same as TUI
- **Chat view** uses system-styled messages with `ConciergeAction` button components
- **Settings:** Concierge section with enabled toggle, detail level dropdown (each option shows description), optional provider/model selectors

### Settings Detail Level Descriptions

Shown beneath the selector when each option is active:

| Selection | Description shown |
|-----------|------------------|
| Quick Hello | "Session title and date with action buttons. No AI call — instant." |
| Session Recap | "AI-generated 1-2 sentence summary of your last session." |
| Smart Triage | "Session summary plus pending tasks, alerts, and unfinished work." |
| Full Briefing | "Complete operational briefing: sessions, tasks, health, gateways, snapshots." |

---

## Files to Create/Modify

### New Files
- `crates/amux-daemon/src/agent/concierge.rs` — ConciergeEngine
- `crates/amux-tui/src/state/concierge.rs` — TUI concierge state
- `crates/amux-tui/src/widgets/concierge.rs` — TUI welcome message rendering
- `frontend/src/components/settings-panel/ConciergeSection.tsx` — settings UI
- `frontend/src/components/ConciergeToast.tsx` — toast notification component

### Modified Files
- `crates/amux-daemon/src/agent/types.rs` — ConciergeConfig, ConciergeDetailLevel, ConciergeActionType, ConciergeAction, AgentEvent variant, pinned flag on AgentThread
- `crates/amux-daemon/src/agent/engine.rs` — Initialize ConciergeEngine
- `crates/amux-daemon/src/agent/llm_client.rs` — Ensure `stream_completion` is pub(crate) accessible
- `crates/amux-daemon/src/server.rs` — Trigger on_client_connected, add ConciergeWelcome to broadcast-always list, handle concierge messages
- `crates/amux-daemon/src/history.rs` — Add `delete_messages(thread_id, message_ids)` method
- `crates/amux-protocol/src/messages.rs` — New message variants for concierge config, AgentDbThread pinned metadata
- `crates/amux-tui/src/client.rs` — Handle ConciergeWelcome events
- `crates/amux-tui/src/app/events.rs` — Route concierge events
- `crates/amux-tui/src/widgets/thread_picker.rs` — Pin concierge thread at top of thread list
- `crates/amux-tui/src/state/settings.rs` — Concierge settings tab/fields
- `frontend/src/lib/agentStore.ts` — Concierge state, actions, ConciergeActionType type
- `frontend/src/components/SettingsPanel.tsx` — Concierge settings section
- `frontend/electron/main.cjs` — IPC handlers for concierge
- `frontend/electron/preload.cjs` — Bridge methods
