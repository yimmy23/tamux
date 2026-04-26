# cmux-next TUI UI Layout Design

**Version:** 0.1.0  
**Date:** 2025-01-XX  
**Status:** Design Draft

This document defines the visual layout, interaction patterns, and component hierarchy for the cmux-next Rust TUI.

---

## Design Philosophy

1. **Terminal-First**: Respect the terminal's constraints and strengths
2. **Keyboard-Native**: All actions accessible via keyboard
3. **Information Dense**: Show maximum relevant state
4. **Progressive Disclosure**: Simple by default, power on demand
5. **Consistent Navigation**: Same patterns across all views

---

## Screen Layout

### Primary Layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ WORKSPACES                                                               │
├──────────────────────────────────────────────────────────────────────────┤
│ ┌────────────────────────────────────┐ ┌────────────────────────────────┐│
│ │                                    │ │                                ││
│ │         PRIMARY PANE               │ │        SECONDARY PANE          ││
│ │         (Variable Width)           │ │        (Variable Width)        ││
│ │                                    │ │                                ││
│ │                                    │ │                                ││
│ │                                    │ │                                ││
│ │                                    │ │                                ││
│ └────────────────────────────────────┘ └────────────────────────────────┘│
├──────────────────────────────────────────────────────────────────────────┤
│ ┌──────────────────────┐ ┌──────────────────────┐ ┌─────────────────────┐│
│ │ GOAL PROGRESS        │ │ TASK QUEUE           │ │ STATUS              ││
│ ├──────────────────────┤ ├──────────────────────┤ ├─────────────────────┤│
│ │ ✓ Step 1             │ │ ● task-abc (run)     │ │ Sessions: 3         ││
│ │ ▶ Step 2             │ │ ○ task-def (queue)   │ │ Approvals: 1        ││
│ │ ○ Step 3             │ │ ○ task-ghi (queue)   │ │ Workspace: 5        ││
│ └──────────────────────┘ └──────────────────────┘ └─────────────────────┘│
├──────────────────────────────────────────────────────────────────────────┤
│ :________________________________________________________________ [APPR] │
└──────────────────────────────────────────────────────────────────────────┘
```

### Layout Dimensions

| Region | Height | Width | Resizable |
|--------|--------|-------|-----------|
| Header Bar | 1 row | 100% | No |
| Primary Pane | Variable | 50-70% | Yes |
| Secondary Pane | Variable | 30-50% | Yes |
| Bottom Panels | 4-8 rows | 33% each | Yes |
| Command Bar | 1 row | 100% | No |

---

## Header Bar

### Components

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [W:main] [W:agent]  │ [S:terminal] [S:chat] [S:logs]  │ goal:fix-bugs ● │
└──────────────────────────────────────────────────────────────────────────┘
```

| Component | Description | Interaction |
|-----------|-------------|-------------|
| Workspace Tabs | `[W:name]` | Click/`Alt+1-9` to switch |
| Surface Tabs | `[S:name]` | Click/`Tab` to switch |
| Active Goal | `goal:name status` | Click to focus goal panel |

### Status Indicators

| Symbol | Meaning |
|--------|---------|
| `●` | Running/in-progress |
| `○` | Queued/pending |
| `✓` | Completed |
| `✗` | Failed |
| `⏸` | Paused |
| `!` | Needs attention (approval required) |

---

## Primary Pane Types

### 1. Terminal Pane

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ● session:abc123 ~ ~/projects/cmux-next                       [x] [split]│
├─────────────────────────────────────────────────────────────────────────┤
│ $ cargo build --release                                                  │
│    Compiling amux-daemon v0.1.10                                         │
│    Compiling amux-protocol v0.1.10                                       │
│    Finished release [optimized] target(s) in 42.5s                      │
│ $ _                                                                      │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2. Agent Thread Pane

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ● thread:fix-tests                                            [x] [split]│
├─────────────────────────────────────────────────────────────────────────┤
│ ┌─ USER ──────────────────────────────────────────────────────────────┐ │
│ │ Fix the failing tests in the authentication module.                 │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│ ┌─ ASSISTANT ─────────────────────────────────────────────────────────┐ │
│ │ I'll investigate the failing tests. Let me search for test files.  │ │
│ │                                                                     │ │
│ │ ▶ TOOL CALL: search_files ─────────────────────────────────────┐    │ │
│ │ │ pattern: "auth.*test"  path: "src"                           │    │ │
│ │ └──────────────────────────────────────────────────────────────┘    │ │
│ │                                                                     │ │
│ │ ◀ TOOL RESULT: 3 matches found                                 │    │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│ ┌─ INPUT ─────────────────────────────────────────────────────────────┐ │
│ │ _                                                                    │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3. Goal Run Detail Pane

```
┌─────────────────────────────────────────────────────────────────────────┐
│ GOAL: Fix authentication test failures                       [pause] [x]│
├─────────────────────────────────────────────────────────────────────────┤
│ Status: RUNNING │ Step: 2/4 │ Duration: 3m 42s │ Replans: 0/3           │
├─────────────────────────────────────────────────────────────────────────┤
│ ┌─ STEP 1: Investigate failures ─────────────────────────────────────┐  │
│ │ ✓ COMPLETED - Found 3 failing tests in auth module                 │  │
│ └─────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│ ┌─ STEP 2: Fix the root cause ───────────────────────────────────────┐  │
│ │ ▶ IN PROGRESS - Task: task-fix-auth-001                            │  │
│ └─────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│ ┌─ STEP 3: Run tests to verify ──────────────────────────────────────┐  │
│ │ ○ PENDING                                                           │  │
│ └─────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│ ┌─ STEP 4: Document the fix ─────────────────────────────────────────┐  │
│ │ ○ PENDING                                                           │  │
│ └─────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4. Task List Pane

```
┌─────────────────────────────────────────────────────────────────────────┐
│ TASKS                                                     [filter: all] │
├─────────────────────────────────────────────────────────────────────────┤
│ ID            │ TITLE              │ STATUS     │ PRI │ CREATED        │
├───────────────┼────────────────────┼────────────┼─────┼────────────────┤
│ task-abc123   │ Analyze logs       │ ● running  │ ▲   │ 2 min ago      │
│ task-def456   │ Run tests          │ ○ queued   │ ●   │ 5 min ago      │
│ task-ghi789   │ Send notification  │ ○ blocked  │ ▼   │ 10 min ago     │
│ task-jkl012   │ Cleanup temp files │ ✓ done     │ ●   │ 1 hour ago     │
│ task-mno345   │ Deploy to staging  │ ✗ failed   │ ▲   │ 2 hours ago    │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Secondary Pane Types

### 1. System Info Panel

```
┌─────────────────────────────────────┐
│ SYSTEM INFO                         │
├─────────────────────────────────────┤
│ Hostname: dev-machine               │
│ OS: Linux 6.1.0                     │
│ CPU: 32 cores │ Load: 2.45          │
│ Memory: ████████░░ 45% (14.4/32 GB)│
│ Disk:   ████████████░ 80%           │
│ Uptime: 14 days, 3 hours            │
└─────────────────────────────────────┘
```

### 2. Memory Panel

```
┌─────────────────────────────────────┐
│ MEMORY                    [SOUL|MEM]│
├─────────────────────────────────────┤
│ # MEMORY.md                         │
│                                     │
│ ## Operator Preferences             │
│ - Prefer concise summaries          │
│ - Show traces before execution      │
│                                     │
│ [Edit] [Save]                       │
└─────────────────────────────────────┘
```

### 3. Skills Browser Panel

```
┌─────────────────────────────────────┐
│ SKILLS                    [search]  │
├─────────────────────────────────────┤
│ ▸ debugging/                        │
│   ├── investigate-test-failure      │
│   └── trace-async-error             │
│ ▸ git/                              │
│   └── smart-commit                  │
│                                     │
│ [Enter] Run  [e] Edit  [n] New      │
└─────────────────────────────────────┘
```

---

## Modal Dialogs

### 1. Approval Modal

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ⚠ APPROVAL REQUIRED                                                     │
├─────────────────────────────────────────────────────────────────────────┤
│ Session: abc123                                                          │
│ Command: rm -rf ./dist                                                   │
│ Security Level: MODERATE                                                 │
│                                                                          │
│ Rationale: Clean the dist directory before rebuilding.                  │
│                                                                          │
│ Risk Assessment:                                                         │
│ • Deletes directory contents                                            │
│ • No network access required                                            │
│                                                                          │
│ [A]pprove    [R]eject    [T]rust Session (yolo)                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2. Command Palette

```
┌─────────────────────────────────────────────────────────────────────────┐
│ :_______________________________________________________________________│
├─────────────────────────────────────────────────────────────────────────┤
│ :sessions                     List active sessions                      │
│ :goals                        List goal runs                            │
│ :workspace-tasks              List workspace tasks                      │
│ :threads                      List agent threads                        │
│ :memory                       Open memory editor                        │
│ :skills                       Browse skill documents                    │
│ :history <query>              Search command history                    │
│ :approve <id>                 Approve pending command                   │
│ :split horizontal             Split pane horizontally                   │
│ :layout main-stack            Apply main-stack layout                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Keyboard Shortcuts

### Global

| Key | Action |
|-----|--------|
| `:` | Open command palette |
| `?` | Show help overlay |
| `Tab` | Next pane |
| `Shift+Tab` | Previous pane |
| `1-9` | Jump to workspace N |
| `q` | Close current pane |

### Session

| Key | Action |
|-----|--------|
| `n` | New session |
| `k` | Kill session |
| `a` | Attach to session |
| `i` | Enter insert mode |
| `Esc` | Exit insert mode |

### Agent

| Key | Action |
|-----|--------|
| `c` | New thread |
| `Enter` | Send message |
| `r` | Toggle reasoning view |

### Goal

| Key | Action |
|-----|--------|
| `p` | Pause goal |
| `r` | Resume goal |
| `x` | Cancel goal |
| `R` | Rerun from step |

---

## Color Scheme

### Semantic Colors

| Purpose | Color | ANSI |
|---------|-------|------|
| Success | Green | 2 |
| Warning | Yellow | 3 |
| Error | Red | 1 |
| Info | Blue | 4 |
| Running | Cyan | 6 |
| Queued | Gray | 8 |
| User message | Blue bg | 44 |
| Tool call | Magenta | 5 |

---

## Component Hierarchy

```
App
├── HeaderBar
│   ├── WorkspaceTabs
│   ├── SurfaceTabs
│   └── ActiveGoalIndicator
├── MainArea
│   ├── PrimaryPane (variable)
│   │   ├── TerminalPane
│   │   ├── AgentThreadPane
│   │   ├── GoalDetailPane
│   │   └── TaskListPane
│   └── SecondaryPane (variable)
│       ├── SystemInfoPanel
│       ├── MemoryPanel
│       └── SkillsBrowserPanel
├── BottomPanels (collapsible)
│   ├── GoalProgressPanel
│   ├── TaskQueuePanel
│   └── StatusPanel
├── CommandBar
└── ModalLayer
    ├── ApprovalModal
    ├── CommandPalette
    └── HelpOverlay
```

---

## Implementation Phases

### Phase 1: Core (P0)
- Session management (list, spawn, attach, kill)
- PTY output rendering
- Command palette
- Basic layout

### Phase 2: Agent Integration (P1)
- Agent thread view with streaming
- Workspace task and execution queue panels
- Goal runner detail view
- Approval modal

### Phase 3: Extended (P2)
- Memory panel
- Skills browser
- History search
- System info

### Phase 4: Polish (P2)
- Responsive layout
- Accessibility
- Theming
- Performance optimization

---

*End of UI Layout Design*
