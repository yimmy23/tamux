# Tamux TUI Design Specification

**Status:** Approved
**Date:** 2026-03-19
**Crate:** `crates/amux-tui` (binary: `tamux-tui`)

## 1. Architecture

### 1.1 Daemon-as-Brain, TUI-as-Skin

The daemon owns all durable state: LLM streams, tool execution, task scheduling, approval policy, and SQLite persistence. The TUI owns only transient view state: focus, scroll offsets, modal visibility, local fuzzy queries, and form buffers.

Communication is unidirectional for events (Daemon → TUI via `ClientEvent`) and command-oriented for mutations (TUI → Daemon via `DaemonCommand`). The TUI never mutates domain state locally — it issues commands and re-renders from daemon projections.

### 1.2 Migration Context

This spec describes a **rewrite** of the existing TUI architecture. The current crate has a flat structure (`app.rs` with a monolithic ~3,500-line `TuiModel`, `client.rs`, `state.rs`, `actions.rs`, `layout.rs`, `modal.rs`, `panes.rs`, `theme.rs`) using `ftui-runtime`'s `StringModel` adapter for line-by-line character painting.

The rewrite replaces the `StringModel` line-painting approach with a proper `ftui-core` widget tree, decomposes the monolithic `TuiModel` into focused state modules, and changes the layout from three-pane (Threads | Chat | Mission) to two-pane (Chat | Sidebar) with threads as a modal picker.

**What is preserved:** `client.rs` (daemon communication) and `state.rs` (wire-format types: `AgentThread`, `AgentMessage`, `AgentTask`, `GoalRun`, etc.) are reused. The existing `FocusArea` variants (`Threads`, `Chat`, `Mission`, `Composer`) are replaced with three: `Chat`, `Sidebar`, `Input`. The existing `panes.rs` and `layout.rs` are replaced entirely by the `widgets/` directory.

Implementation must include a **Phase 0** that sets up the new module structure and migrates the entry point before any feature work.

### 1.3 Elm-Delegated + Projection Layer

The architecture combines `ftui-runtime`'s Elm-style update/view loop with decomposed state modules and a projection layer:

```
frankentui event loop
┌───────────┐    ┌──────────┐   ┌───────────┐
│ crossterm  │───▶│ TuiModel │──▶│ Widget    │
│ events     │    │ .update()│   │ tree      │
└───────────┘    └────┬─────┘   └───────────┘
                      │
         ┌────────────┼────────────┐
         ▼            ▼            ▼
   ┌──────────┐ ┌──────────┐ ┌──────────┐
   │ChatState │ │TaskState │ │ModalState│
   │.reduce() │ │.reduce() │ │.reduce() │
   └──────────┘ └──────────┘ └──────────┘
         ▲            ▲            ▲
         └────────────┼────────────┘
                      │
            ┌─────────┴──────────┐
            │ DaemonProjection   │
            │ raw events →       │
            │   typed actions    │
            └─────────┬──────────┘
                      │
            ┌─────────┴──────────┐
            │  DaemonBridge      │
            │  (client.rs)       │
            └────────────────────┘
```

**DaemonProjection** is a pure function: `ClientEvent → Vec<AppAction>`. It translates wire-format events into typed UI actions, keeping state modules decoupled from the transport layer.

**State modules** are plain structs with two methods:
- `reduce(&mut self, action)` — pure state transition, no side effects.
- `effects(&self, action) -> Vec<DaemonCommand>` — side effects to send to daemon.

**Widget functions** are pure: `(&State, &Theme) → Element`. No I/O, no mutation.

**Root model (TuiModel)** composes all state modules and delegates. The `update()` method pumps daemon events through the projection, maps crossterm events to actions, dispatches to sub-modules, and collects effects. The `view()` method builds the widget tree from state slices.

### 1.4 Daemon Client & Bridge

Reuse the existing `client.rs` unchanged. It handles:
- Unix socket + TCP fallback with WSL IP auto-detection.
- `amux-protocol`'s `AmuxCodec` framing.
- All daemon message types (threads, tasks, goal runs, streaming events, config, approvals, heartbeat).

**Channel architecture:** The `DaemonClient` internally sends/receives `ClientMessage` (the wire protocol type from `amux-protocol`). The bridge thread in `main.rs` translates between the TUI's `DaemonCommand` enum and the protocol's `ClientMessage` enum. From the TUI model's perspective, the interface is:
- Inbound: `mpsc::Sender<ClientEvent>` (daemon → TUI, typed events)
- Outbound: `tokio_mpsc::UnboundedSender<DaemonCommand>` (TUI → bridge → `ClientMessage`)

## 2. Module Structure

```
crates/amux-tui/src/
├── main.rs                  # Entry point: daemon bridge thread + ftui App
├── app.rs                   # TuiModel compositor + Msg + StringModel impl
├── client.rs                # DaemonClient (reused as-is)
├── projection.rs            # DaemonProjection: ClientEvent → AppAction
│
├── state/
│   ├── mod.rs               # Re-exports + AppAction enum
│   ├── chat.rs              # ChatState: threads, messages, streaming, scroll
│   ├── sidebar.rs           # SidebarState: tab, selection, scroll, expanded nodes
│   ├── input.rs             # InputState: buffer, mode, cursor, multiline
│   ├── modal.rs             # ModalState: stack, command_query, picker cursors
│   ├── config.rs            # ConfigState: provider, model, api_key, tools, effort
│   ├── approval.rs          # ApprovalState: pending approvals, risk, blast radius
│   ├── task.rs              # TaskState: tasks, goal_runs, heartbeats, subagents
│   └── settings.rs          # SettingsState: tab, field cursor, edit buffers, dirty
│
├── widgets/
│   ├── mod.rs               # Re-exports
│   ├── header.rs            # header_widget(): logo, agent label, model, tokens
│   ├── footer.rs            # footer_widget(): input prompt + shortcut hints
│   ├── chat.rs              # chat_widget(): message list with streaming
│   ├── message.rs           # message_widget(): single message (user/asst/tool)
│   ├── reasoning.rs         # reasoning_widget(): collapsible thinking block
│   ├── sidebar.rs           # sidebar_widget(): tabs + task tree / subagents
│   ├── task_tree.rs         # task_tree_widget(): nested dependency tree
│   ├── subagents.rs         # subagents_widget(): hierarchy + progress
│   ├── command_palette.rs   # command_palette_widget(): fuzzy finder overlay
│   ├── approval.rs          # approval_widget(): risk modal overlay
│   ├── thread_picker.rs     # thread_picker_widget(): thread list modal
│   ├── settings.rs          # settings_widget(): 6-tab configuration panel
│   ├── provider_picker.rs   # provider_picker_widget(): LLM provider selector
│   ├── model_picker.rs      # model_picker_widget(): model list selector
│   └── splash.rs            # splash_widget(): ASCII art empty state
│
└── theme.rs                 # Color palette, border styles, focus ring tokens
```

## 3. State Modules

### 3.1 ChatState

```rust
pub struct ChatState {
    threads: Vec<AgentThread>,
    active_thread_id: Option<String>,
    streaming_content: String,
    streaming_reasoning: String,
    active_tool_calls: Vec<ToolCallVm>,
    scroll_offset: usize,           // 0 = following tail
    scroll_locked: bool,
    transcript_mode: TranscriptMode, // Compact | Tools | Full
}

pub enum ChatAction {
    Delta { thread_id: String, content: String },
    Reasoning { thread_id: String, content: String },
    ToolCall { thread_id: String, call_id: String, name: String, args: String },
    ToolResult { thread_id: String, call_id: String, name: String, content: String, is_error: bool },
    TurnDone {
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
    },
    ThreadListReceived(Vec<AgentThread>),
    ThreadDetailReceived(AgentThread),
    ThreadCreated { thread_id: String, title: String },
    SelectThread(String),
    ScrollChat(i32),
    NewThread,
    SetTranscriptMode(TranscriptMode),
}
```

### 3.2 TaskState

```rust
pub struct TaskState {
    tasks: Vec<AgentTask>,
    goal_runs: Vec<GoalRun>,
    heartbeat_items: Vec<HeartbeatItem>,
}

pub enum TaskAction {
    TaskListReceived(Vec<AgentTask>),
    TaskUpdate(AgentTask),
    GoalRunListReceived(Vec<GoalRun>),
    GoalRunDetailReceived(GoalRun),
    GoalRunUpdate(GoalRun),
    HeartbeatItemsReceived(Vec<HeartbeatItem>),
}
```

### 3.3 SidebarState

```rust
pub struct SidebarState {
    active_tab: SidebarTab,     // Tasks | Subagents
    selected_item: usize,
    scroll_offset: usize,
    expanded_nodes: HashSet<String>,
}

pub enum SidebarAction {
    SwitchTab(SidebarTab),
    Navigate(i32),
    ToggleExpand(String),
    Scroll(i32),
}
```

### 3.4 InputState

```rust
pub struct InputState {
    buffer: String,
    mode: InputMode,            // Normal | Insert
    cursor_pos: usize,
    multiline: bool,
}

pub enum InputAction {
    InsertChar(char),
    Backspace,
    Submit,
    ToggleMode,
    Clear,
    InsertNewline,
}
```

### 3.5 ModalState

```rust
pub struct ModalState {
    stack: Vec<ModalKind>,
    command_query: String,
    command_items: Vec<CommandItem>,
    filtered_items: Vec<usize>,
    picker_cursor: usize,
}

pub enum ModalAction {
    Push(ModalKind),
    Pop,
    SetQuery(String),
    Navigate(i32),
    Execute,
    FuzzyFilter,
}
```

### 3.6 ConfigState

```rust
pub struct ConfigState {
    provider: String,
    base_url: String,
    model: String,
    api_key: String,
    reasoning_effort: String,
    fetched_models: Vec<FetchedModel>,
    agent_config_raw: Option<Value>,
}

pub enum ConfigAction {
    ConfigReceived(AgentConfigSnapshot),
    ConfigRawReceived(Value),
    ModelsFetched(Vec<FetchedModel>),
    SetProvider(String),
    SetModel(String),
}
```

### 3.7 ApprovalState

```rust
pub struct ApprovalState {
    pending_approvals: Vec<PendingApproval>,
    session_allowlist: HashSet<String>,
}

/// RiskLevel is a TUI-side enum parsed from the protocol's string field.
/// Mapping: "low" → Low, "medium" → Medium, "high" → High, "critical" → Critical.
/// Unknown strings default to Medium.
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

pub struct PendingApproval {
    approval_id: String,
    task_id: String,
    task_title: Option<String>,
    /// Extracted from AgentTask.blocked_reason or goal step context.
    /// May be empty if daemon does not provide command text.
    command: String,
    risk_level: RiskLevel,
    blast_radius: String,
}

pub enum ApprovalAction {
    /// Derived from TaskState: when a task has awaiting_approval_id set,
    /// the TUI constructs a PendingApproval from available task metadata.
    ApprovalRequired(PendingApproval),
    Resolve { approval_id: String, decision: String },
    AllowSession(String),
}
```

### 3.8 SettingsState

```rust
pub struct SettingsState {
    active_tab: SettingsTab,
    field_cursor: usize,
    editing_field: Option<String>,
    dropdown_open: bool,
    dropdown_cursor: usize,
    dirty: bool,
}

pub enum SettingsTab {
    Provider,
    Model,
    Tools,
    Reasoning,
    Gateway,
    Agent,
}

pub enum SettingsAction {
    Open,
    Close,
    SwitchTab(SettingsTab),
    NavigateField(i32),
    EditField,
    ConfirmEdit,
    CancelEdit,
    ToggleCheckbox,
    SelectRadio,
    OpenDropdown,
    NavigateDropdown(i32),
    SelectDropdown,
    Save,
}
```

## 4. Visual Language

### 4.1 Branding

Logo (gradient wave):
```
░▒▓█ T A M U X █▓▒░
     plan · solve · ship
```

Appears as centered splash when chat is empty, and as compact inline form in the header bar.

### 4.2 Color Palette

| Token | Hex | ANSI-256 | Usage |
|---|---|---|---|
| `bg.main` | Terminal default | `Color::Reset` | Background |
| `fg.dim` | #8b949e | `Indexed(245)` | Inactive text, borders |
| `fg.active` | #e6edf3 | `Indexed(255)` | Bright active text |
| `accent.primary` | #58a6ff | `Indexed(75)` | Focus ring, user messages |
| `accent.assistant` | #d2a8ff | `Indexed(183)` | Assistant messages, tool badges |
| `accent.secondary` | #e3b341 | `Indexed(178)` | Warnings, menu highlights, modals |
| `accent.success` | #3fb950 | `Indexed(78)` | Completed, connected, OK |
| `accent.danger` | #f85149 | `Indexed(203)` | Errors, critical risk, failed |

### 4.3 Border Styles

- **Persistent panes:** Rounded — `╭─╮ │ │ ╰─╯`
- **Modal overlays:** Sharp/double — `╔═╗ ║ ║ ╚═╝`
- **Focus ring:** Focused pane border = `accent.primary` (cyan), title = `fg.active`. Unfocused = `fg.dim`.

### 4.4 Density

- Default views stay compact.
- Verbose internals (reasoning, tool args/results) are collapsible.
- Transient actions use floating overlays to reduce clutter.

## 5. Layout

### 5.1 Two-Pane Structure

```
Column {
  ├─ header_widget()                 // height: 3 (border + content + border)
  │    Row { logo + agent | model | tokens }
  │
  ├─ Row { flex: 1 }                // main area
  │    ├─ chat_widget()   flex: 7   // ~65%
  │    └─ sidebar_widget() flex: 3  // ~35%
  │
  ├─ footer_widget()                // height: 4 (border + input + hints + border)
  │    Column { input_line, shortcuts_line }
  │
  └─ overlay (conditional)          // floats above everything
       ├─ command_palette_widget()
       ├─ approval_widget()
       ├─ thread_picker_widget()
       ├─ settings_widget()
       ├─ provider_picker_widget()
       └─ model_picker_widget()
}
```

Thread list is a modal picker (`Ctrl+T`), not a persistent pane.

### 5.2 Responsive Breakpoints

| Terminal Width | Layout | Sidebar |
|---|---|---|
| ≥ 120 cols | Full two-pane (65/35) | Visible, full task tree |
| 100–119 cols | Compressed two-pane (70/30) | Visible, compact mode |
| 80–99 cols | Single pane + sidebar toggle | Hidden, `Ctrl+B` to toggle overlay |
| < 80 cols | Single pane only | Via command palette only |

## 6. Core Components

### 6.1 Chat & Message Rendering

Messages are rendered as blocks in a vertical scroll list:

- **User:** Cyan badge `USER`, bright white content, 7-char indent for wrap alignment.
- **Assistant:** Lavender badge `ASST`, bright white content, inline reasoning and tool indicators.
- **System:** Grey badge `SYS`, dim text. Turn summaries with provider/model/tokens/TPS/cost.
- **Tool:** Gear icon `⚙`, tool name, status badge, elapsed time.

Tool call states:
- Compact (default): single line — `⚙ tool_name  ✓ done 1.2s`
- Expanded (on select): shows args + truncated result (5 lines max).
- Running: braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` cycling on tick.

### 6.2 Collapsible Reasoning

- Collapsed (default): `▸ [+] Reasoning (12s · 847 tok)` in dim text.
- Expanded: `▾ [-] Reasoning (...)` with dark blue left border, dim reasoning text.
- Toggle with `r` key or Enter when selected.
- During streaming: auto-expands while reasoning arrives, auto-collapses when content stream begins.

### 6.3 Streaming Behavior

- **Auto-scroll:** Chat anchors to bottom while streaming (scroll_offset = 0 means "following tail").
- **Scroll lock:** User scroll-up sets `scroll_locked = true`. New content arrives but view stays. End key or scroll-to-bottom unlocks.
- **Cursor indicator:** Block cursor `█` appended during stream, removed on `Done` event.
- **Turn complete:** Dim system line with provider, model, token counts, TPS, cost.

### 6.4 Transcript Modes

- **Compact** (default): Merged tool rows, concise assistant lines.
- **Tools**: Tool calls only with args preview (debugging agent behavior).
- **Full**: Everything expanded — reasoning, tool args, tool results.

Cycled via `/view compact|tools|full`.

### 6.5 Context Sidebar — Tasks Tab

Three zones:
1. **Goal Runs** — Collapsible tree with `▾/▸`. Steps as children with status chips: `[ ]` pending, `[~]` running (amber), `[x]` done (green), `[!]` failed (red). Replan attempts visible as nested items.
2. **Standalone Tasks** — Flat list with priority badges (`▲` high, `●` normal, `▼` low). Blocked tasks show `[B]` with reason.
3. **Heartbeat** — Health indicator dots: green OK, amber warning, red error.

### 6.6 Context Sidebar — Subagents Tab

Grouped by runtime (hermes, openclaw, daemon). Each shows:
- Classification tag (coding, research, ops, browser, messaging).
- Child runs with status dot and name.
- Linked thread, duration, token count.
- Enter on a subagent opens its chat thread.
- Footer shows aggregate counts.

### 6.7 Command Palette

Triggered by `Ctrl+P` or `/` as first character in input.

Sharp amber border (`╔═╗`). Fuzzy search input at top. Selected item highlighted in amber with black text. j/k or arrows to navigate, Enter to execute, Esc to dismiss.

Commands:
| Command | Action | Opens |
|---|---|---|
| `/provider` | Switch LLM provider | Provider picker |
| `/model` | Switch model | Model picker (fetches from daemon) |
| `/tools` | Toggle tool sets | Checklist picker |
| `/effort` | Set reasoning effort | List: off/minimal/low/med/high/xhigh |
| `/thread` | Pick conversation thread | Thread picker (same as Ctrl+T) |
| `/new` | New conversation | — |
| `/goal` | Start a goal run | Goal prompt input |
| `/view` | Switch transcript mode | List: compact/tools/full |
| `/settings` | Open settings panel | Full settings overlay |
| `/prompt` | Edit system prompt | Text editor overlay |
| `/quit` | Exit TUI | — |

Sub-commands push secondary pickers onto the modal stack.

### 6.8 Settings Panel

Full modal overlay triggered by `/settings` or `Ctrl+,`. Six tabs:

1. **Provider** — Dropdown with 20+ providers, base URL input, masked API key with `[show]` toggle.
2. **Model** — Fetches available models from daemon, context window display, list picker.
3. **Tools** — Checkbox toggles for 6 tool categories with sub-tool descriptions.
4. **Reasoning** — Radio buttons for effort level, numeric fields for max tool loops/retries/context budget.
5. **Gateway** — Enable toggle, command prefix, per-platform token fields with connection status.
6. **Agent** — Name, handler prefix, system prompt textarea, backend runtime selector.

Navigation: Tab/Shift+Tab cycles tabs, j/k navigates fields, Enter edits, Space toggles, Esc closes (auto-saves), Ctrl+S saves without closing.

Field types: text input, secret (masked), dropdown, checkbox, radio, number, textarea.

### 6.9 Approval Modal

Focus-trapping overlay. Border color reflects risk: red for HIGH/CRITICAL, amber for MEDIUM. Dimmed `░` backdrop.

Content: risk level badge, exact command, blast radius, source task.

Actions — single-key, no navigation needed:
- `Y` — Allow once
- `A` — Allow for session (adds to session allowlist)
- `N` — Reject

Sends `DaemonCommand::ResolveTaskApproval { approval_id, decision }`.

**Data source:** The daemon does not currently push a dedicated `ApprovalRequired` event with command text and risk metadata. Instead, pending approvals are derived from `AgentTask` records where `awaiting_approval_id` is set. The `blocked_reason` field may contain the command text. Risk level and blast radius must be inferred TUI-side from the command pattern (same heuristics as the frontend's `agentMissionStore`), or the daemon protocol should be extended to include `risk_level` and `blast_radius` fields on `AgentTask`. For Phase 2 implementation, the TUI will use heuristic classification; a protocol extension is recommended for Phase 4.

### 6.10 Thread Picker

Triggered by `Ctrl+T` or `/thread`. Sharp amber border. Search filters by title. Green dot = active/streaming, grey dot = idle. Shows time ago and token count. First item always "+ New conversation". Enter selects and loads thread history from daemon.

### 6.11 Splash Screen

Centered in chat pane when no active thread:

```
         ░▒▓█ T A M U X █▓▒░
              plan · solve · ship

         Type a prompt to begin, or
         Ctrl+P to open command palette
         Ctrl+T to pick a thread
```

Gradient wave logo with cyan-to-white color ramp. Hints in dim text.

## 7. Interaction Model

### 7.1 Focus Management

Focus areas cycle with Tab / Shift+Tab: **Chat → Sidebar → Input → Chat**.

When a modal is open, focus traps inside the modal. Esc dismisses and returns focus to the previous area.

Visual indicators:
- Focused pane: border = `accent.primary` (cyan), title = `fg.active` (bright white).
- Unfocused pane: border = `fg.dim` (grey), title = `fg.dim`.
- Modal: border = `accent.secondary` (amber).
- Approval modal: border = `accent.danger` (red) for HIGH/CRITICAL.

### 7.2 Input Modes

- **Normal mode** — Vim-like navigation. j/k scroll, `/` opens command palette, `i`/Enter enters Insert mode.
- **Insert mode** — Composing messages. Enter sends, `\+Enter` inserts newline, Esc returns to Normal.
- **Modal mode** — Focus trapped in overlay. j/k navigate items, Enter selects, Esc closes.

### 7.3 Keyboard Map

#### Global (all modes)
| Key | Action |
|---|---|
| `Tab / Shift+Tab` | Cycle focus: Chat → Sidebar → Input |
| `Ctrl+P` | Open command palette |
| `Ctrl+T` | Open thread picker |
| `Ctrl+,` | Open settings |
| `Esc` | Close modal / switch to Normal mode |

#### Normal Mode
| Key | Action |
|---|---|
| `j / k / ↑ / ↓` | Scroll focused pane |
| `Ctrl+D / Ctrl+U` | Page down / page up |
| `G / gg` | Jump to bottom / top of chat |
| `i / Enter` | Switch to Insert mode |
| `/` | Open command palette with slash prefix |
| `r` | Toggle reasoning on selected message |
| `[ / ]` | Switch sidebar tab (Tasks ↔ Subagents) |
| `q` | Quit (confirmation if streaming) |

#### Insert Mode
| Key | Action |
|---|---|
| `Enter` | Send message |
| `\ + Enter` | Insert newline (multiline) |
| `Esc` | Switch to Normal mode |
| `/` (first char) | Opens command palette |

#### Modal
| Key | Action |
|---|---|
| `j / k / ↑ / ↓` | Navigate list items |
| `Enter` | Select / execute |
| `Esc` | Close modal |

#### Approval Modal
| Key | Action |
|---|---|
| `Y` | Allow once |
| `A` | Allow for session |
| `N` | Reject |

## 8. Event Loop

Every tick (~16ms):

1. **Pump daemon events** — `while let Ok(event) = daemon_rx.try_recv()`: project through `DaemonProjection`, dispatch typed actions to sub-modules, collect effects → `daemon_cmd_tx`.

2. **Map crossterm event** — Based on `(input_mode, focus, modal_stack.top())`, produce `AppAction`, route to sub-module `.reduce()`.

3. **Build widget tree** (pure) — Compose widgets from state slices. If modal active, wrap in overlay.

4. **Diff & render** — frankentui handles diffing and only updates changed terminal cells.

## 9. Implementation Phases

### Phase 0: Architectural Migration
- Set up new module structure (`state/`, `widgets/`, `projection.rs`).
- Migrate `main.rs` entry point to use new `TuiModel` compositor.
- Preserve `client.rs` and `state.rs` (wire types) unchanged.
- Remove old modules: `panes.rs`, `layout.rs`, old `modal.rs`, old `theme.rs`, old `actions.rs`.
- Establish new `ThemeTokens` with the retro-hacker color palette.
- Verify the crate compiles and connects to daemon (blank screen OK).

### Phase 1: Shell & Chat
- Crate skeleton with widget tree rendering (replace StringModel line painting).
- Theme tokens and focus ring.
- Header, footer, two-pane layout.
- Splash screen with logo.
- ChatState + message widget (user/assistant/system).
- Streaming with auto-scroll and cursor indicator.
- InputState with Normal/Insert mode switching.
- DaemonProjection for Delta/Reasoning/ToolCall/Done events.

### Phase 2: Modals & Interaction
- ModalState with stack.
- Command palette with fuzzy search.
- Thread picker.
- Approval modal with Y/A/N handling.
- Collapsible reasoning blocks.
- Tool call indicators (compact + expanded).
- Transcript modes (compact/tools/full).

### Phase 3: Sidebar & Tasks
- SidebarState with tab switching.
- TaskState integration.
- Task tree widget with goal run nesting.
- Subagents widget with runtime grouping.
- Heartbeat indicators.
- `[/]` tab switching shortcuts.

### Phase 4: Settings & Polish
- SettingsState with 6 tabs.
- All field types (text, secret, dropdown, checkbox, radio, number, textarea).
- ConfigState ↔ settings panel bidirectional sync.
- Provider picker, model picker sub-modals.
- Responsive breakpoints (sidebar collapse).
- Vim motions: `G/gg`, `Ctrl+D/U`.
- Mouse support (click to focus, scroll).

## 10. Acceptance Criteria

1. No raw JSON in default chat mode.
2. Streaming remains smooth during long tool runs (auto-scroll + scroll-lock).
3. Approval modal is fully keyboard-operable (Y/A/N).
4. Command palette supports fuzzy query with real-time filtering.
5. Task tree correctly renders goal → step → task hierarchy.
6. Settings panel can configure provider/model/tools/reasoning and persist via daemon.
7. Thread picker loads history from daemon on selection.
8. Sidebar collapses gracefully below 100 columns.
9. All state modules are unit-testable (pure reduce functions).
10. Widget functions are pure (no I/O, deterministic output from state).
