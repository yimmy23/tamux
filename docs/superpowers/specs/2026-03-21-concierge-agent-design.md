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
    pub thread_id: Option<String>,               // auto-assigned, persisted
}
```

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

### New AgentEvent Variant

```rust
AgentEvent::ConciergeWelcome {
    thread_id: String,
    content: String,
    detail_level: String,
    actions: Vec<ConciergeAction>,
}
```

### ConciergeAction

Structured quick-action buttons rendered alongside the greeting text:

```rust
pub struct ConciergeAction {
    pub label: String,              // "Continue: auth middleware refactor"
    pub action_type: String,        // "continue_session" | "start_new" | "search" | "dismiss"
    pub thread_id: Option<String>,  // for continue_session
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
    thread_id: RwLock<Option<String>>,
    pending_welcome_ids: RwLock<Vec<String>>,
}
```

### Lifecycle

1. **Daemon starts** — `ConciergeEngine::new()` alongside `AgentEngine::new()`
2. **`concierge.initialize()`** — Load or create pinned concierge thread, persist `thread_id` in config
3. **Client connects** (`AgentSubscribe` received) — `concierge.on_client_connected()`
4. **Context gathering** — Based on `detail_level`, query history/tasks/health
5. **Compose and emit** — Welcome message + actions via `AgentEvent::ConciergeWelcome`
6. **Ongoing** — Handle messages to the concierge thread via cheap model with ops tools

### LLM Calls

The concierge makes its own LLM calls directly via `http_client` — it does not go through `AgentEngine.send_message_inner()`. This avoids competing with the main agent, keeps the concierge system prompt separate, and allows a different provider/model without per-task override machinery.

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
| **ProactiveTriage** | + `engine.list_tasks()` + heartbeat items + pending approvals | Yes — summary + actionable triage |
| **DailyBriefing** | + health status + gateway connectivity + snapshot stats + time since last activity | Yes — full operational briefing |

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
| Disconnect without interaction | Pruned on next connect before generating new welcome. |

### Message Pruning (Not Thread Deletion)

The concierge thread is permanent. Only transient welcome messages are pruned. Track IDs in `ConciergeEngine.pending_welcome_ids`. On any cleanup trigger, remove those messages and clear the vec.

### Reconnection

On connect with stale welcome from previous session:
1. Prune old welcome messages
2. Gather fresh context
3. Emit new welcome

The concierge thread never accumulates ignored greetings.

### Main Agent Handoff

When user asks something beyond concierge capability:
1. Concierge: "That's a job for the main agent. Let me take you there."
2. Emit `navigate_to_session` for main thread (or create new)
3. Optionally prepend user's request as first message in target thread

---

## UI Surface

### TUI

- **Sidebar:** Concierge thread pinned at top with distinct label (e.g., `◆ Concierge`)
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
- `crates/amux-daemon/src/agent/types.rs` — ConciergeConfig, ConciergeDetailLevel, ConciergeAction, AgentEvent variant, pinned flag
- `crates/amux-daemon/src/agent/engine.rs` — Initialize ConciergeEngine
- `crates/amux-daemon/src/server.rs` — Trigger on_client_connected, handle concierge messages
- `crates/amux-protocol/src/messages.rs` — New message variants for concierge config
- `crates/amux-tui/src/client.rs` — Handle ConciergeWelcome events
- `crates/amux-tui/src/app/events.rs` — Route concierge events
- `crates/amux-tui/src/widgets/sidebar.rs` — Pin concierge thread at top
- `crates/amux-tui/src/state/settings.rs` — Concierge settings tab/fields
- `frontend/src/lib/agentStore.ts` — Concierge state and actions
- `frontend/src/components/SettingsPanel.tsx` — Concierge settings section
- `frontend/electron/main.cjs` — IPC handlers for concierge
- `frontend/electron/preload.cjs` — Bridge methods
