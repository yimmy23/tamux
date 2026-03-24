# Frankentui Super-Intelligent TUI Spec

Status: Implementation blueprint
Date: 2026-03-19

## 1. Architecture Contract

Daemon-as-Brain, TUI-as-Skin:
1. Daemon owns canonical state, LLM streams, tool execution, approval policy, and SQLite persistence.
2. TUI owns view state only: focus, pane split, modal visibility, scroll offsets, local fuzzy query, transient form state.
3. IPC is unidirectional for events (Daemon -> TUI) and command-oriented for actions (TUI -> Daemon).
4. TUI never mutates durable domain state locally. It issues commands and re-renders from daemon projections.

## 2. Visual Language (Modern Retro-Hacker)

Typography and branding:
1. Hero/empty states use block ASCII logos.
2. Pane headers use compact all-caps labels with low-noise metadata.
3. Dense body text uses dim white by default with bright white for active content.

Color tokens (frankentui tags):
1. bg.main: black or very dark gray.
2. fg.text: dim white.
3. fg.active: bright white.
4. accent.primary: neon cyan/teal.
5. accent.secondary: warm orange/peach.
6. accent.warn: bright red.

Borders:
1. Persistent panes use rounded border set: ╭─╮ ╰─╯.
2. Floating overlays use sharp/high-contrast borders.
3. Focus ring upgrades pane border to primary accent.

Density rules:
1. Default views stay compact.
2. Verbose internals move into subviews or expandable blocks.
3. Overlay modals handle transient actions to reduce baseline clutter.

## 3. Global Layout (Flex)

Top bar (height 1):
1. Left: active agent/session label.
2. Center: active model label.
3. Right: token and cost budget counters.

Main area (flex 1):
1. Left pane: chat and command-centric surface (60-70%).
2. Right pane: context sidebar (tasks/subagents/file context) (30-40%).
3. Split ratio adjustable and persisted as view preference.

Bottom bar (height 2):
1. Input row: prompt and multiline composer state.
2. Shortcut row: context-sensitive controls and mode hints.

## 4. Core Components

### 4.1 Chat and Reasoning View

Requirements:
1. User, Assistant, and Tool rows are visually distinct.
2. Streaming anchors to bottom unless user explicitly scroll-locks.
3. Tool calls render as status badges with subtle progress indicators.
4. Reasoning is collapsed by default and expanded on demand.

Reasoning pattern:
1. Compact row: [+] Reasoning (45s)
2. Expanded block: left accent border + wrapped body + elapsed metadata.

Transcript modes:
1. compact: merged tool rows and concise assistant lines.
2. tools: tool rows only.
3. full: full verbose rows for debugging.

### 4.2 Context Sidebar: Tasks and Subagents

Tabs:
1. [Tasks]
2. [Subagents]

Tasks view:
1. Tree structure supports goal -> step -> task relationships.
2. Status chips: [ ] pending, [~] running, [x] done, [!] failed.
3. Replan attempts and nested dependencies are visible but collapsible.

Subagents view:
1. Hierarchical list by parent agent.
2. Per-agent state, active tool, and progress indicator.
3. Last heartbeat age and warning color for stale workers.

### 4.3 Approval Alert Modal

Behavior:
1. Critical approvals interrupt with centered floating modal.
2. Modal contains command, risk level, and blast radius.
3. Actions: allow once, allow for session, reject.

Visual:
1. Thick warning border (orange/red).
2. High-contrast action row with selected option focus style.

### 4.4 Command Palette Modal

Trigger:
1. Ctrl+P
2. Slash input in composer (/)

Behavior:
1. Fuzzy search input at top.
2. Left list: command names.
3. Right preview: description, side effects, and hotkeys.
4. Selection navigation via j/k or arrows.

Command families:
1. /provider
2. /model
3. /tools
4. /effort
5. /view chat ...
6. /view mission ...

## 5. Interaction Model

Keyboard-first:
1. j/k and arrows: list navigation.
2. ctrl+d and ctrl+u: viewport page jumps.
3. tab and shift+tab: focus cycle.
4. enter: execute command or send prompt.
5. backslash + enter: insert newline in composer.

Mouse support (optional but complete):
1. Click pane to focus.
2. Wheel scroll per focused pane.
3. Click overlay actions.

Focus ring:
1. Active pane border switches to primary accent.
2. Inactive panes remain dimmed.

## 6. IPC Surface (Suggested)

Daemon -> TUI events:
1. Session projection updates.
2. Token stream delta chunks.
3. Tool call lifecycle updates.
4. Task tree and subagent snapshots.
5. Approval-required interrupts.
6. Error and telemetry events.

TUI -> Daemon commands:
1. SubmitPrompt
2. ExecuteSlashCommand
3. ControlGoalRun
4. ResolveApproval
5. UpdateAgentConfig
6. RequestProjectionRefresh

## 7. Event Loop Mapping (Crossterm)

1. Key events map to high-level UiAction values.
2. UiAction reducer updates transient view state.
3. Side effects dispatch IPC commands.
4. Incoming daemon events update projections.
5. Render pass is declarative over current state + projection.

Reducer intent:
1. Pure state transition functions for deterministic tests.
2. Side-effect separation for IPC and timers.
3. Deterministic replay support for golden snapshots.

## 8. Build Plan

1. Introduce frankentui shell skeleton (header/main/footer + modals).
2. Add theming/token map and focus ring behavior.
3. Integrate chat/task/subagent component renderers.
4. Wire approval and command modal logic.
5. Connect reducer + crossterm + daemon IPC adapter.
6. Add deterministic renderer goldens for major states.

## 9. Acceptance Criteria

1. No raw JSON spam in default chat mode.
2. Streaming remains smooth at tail during long tool runs.
3. Approval modal can be fully operated from keyboard.
4. Command palette supports fuzzy query and preview panel.
5. Task/subagent sidebar remains responsive under high event throughput.
6. Golden snapshots cover header, chat, task tree, command modal, and approval modal.
