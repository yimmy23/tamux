# cmux-next TUI Capabilities Map

**Version:** 0.1.0  
**Date:** 2025-01-XX  
**Status:** Design Draft

This document maps all capabilities that need to be exposed in the Rust TUI for cmux-next, based on comprehensive codebase analysis.

---

## Executive Summary

The cmux-next TUI will expose the following major capability domains:

| Domain | Priority | Complexity | Description |
|--------|----------|------------|-------------|
| Session Management | P0 | Medium | Terminal session lifecycle (spawn, attach, clone, kill) |
| Command Execution | P0 | High | Managed commands with approval gates, queuing |
| Goal Runners | P1 | High | Multi-step autonomous goal execution with planning |
| Workspace Tasks | P1 | Medium | Board-owned tasks over threads and goals |
| Execution Queue | P1 | Medium | Background execution scheduling, dependencies, priorities |
| Agent Threads | P1 | Medium | Conversation threads with LLM, tool calls, history |
| Memory Management | P2 | Low | Persistent memory (SOUL.md, MEMORY.md, USER.md) |
| Skill Workflows | P2 | Medium | Reusable procedural documents, generation |
| Workspace/Surface/Pane | P1 | Medium | Hierarchical layout management |
| History & Search | P2 | Low | Command logs, transcript index, FTS search |
| Gateway Messaging | P2 | Low | Slack/Discord/Telegram/WhatsApp notifications |
| System Observability | P2 | Low | System info, process list, telemetry |

---

## 1. Session Management

### 1.1 Core Operations

| Operation | ClientMessage | Description | TUI Exposure |
|-----------|---------------|-------------|--------------|
| Spawn Session | `SpawnSession` | Create new PTY session | `:new [shell] [--cwd path] [--workspace id]` |
| Clone Session | `CloneSession` | Fork existing session | `:clone <source-id>` |
| Kill Session | `KillSession` | Terminate PTY | `:kill <id>` |
| List Sessions | `ListSessions` | Show all active | `:sessions` |
| Attach Session | Via bridge | Connect to PTY | `:attach <id>` |
| Resize | `Resize` | Terminal dimensions | Auto on window resize |

### 1.2 Session Events (DaemonMessage)

```
SessionSpawned { id, cols, rows, cwd, workspace_id }
SessionExited  { id, exit_code }
Output         { id, data }
CwdChanged     { id, cwd }
CommandStarted { id, command }
CommandFinished { id, exit_code }
```

### 1.3 TUI Views

- **Session List View**: Table of sessions with ID, CWD, alive status, workspace
- **Session Focus Mode**: Full PTY output with command input
- **Session Status Bar**: Current CWD, exit code, duration

---

## 2. Command Execution

### 2.1 Managed Commands

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Queue Managed Command | `ExecuteManagedCommand` | Enqueue for serial execution |
| Resolve Approval | `ResolveApproval` | Approve/reject pending command |

### 2.2 Approval Flow

```
ManagedCommandQueued → ManagedCommandStarted → [ApprovalRequired] → ManagedCommandFinished
```

### 2.3 ManagedCommandRequest Fields

```rust
struct ManagedCommandRequest {
    command: String,
    cwd: Option<String>,
    session: Option<String>,
    security_level: SecurityLevel,  // highest, moderate, lowest, yolo
    sandbox_enabled: bool,
    allow_network: bool,
    language_hint: Option<String>,
    rationale: Option<String>,
}
```

### 2.4 TUI Views

- **Command Queue Panel**: Show queued/running/completed commands
- **Approval Modal**: Display risky command, rationale, approve/reject buttons
- **Command History**: Scrollable list with exit codes, duration, timestamps

---

## 3. Goal Runners

### 3.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Start Goal Run | `AgentStartGoalRun` | Begin multi-step execution |
| List Goal Runs | `AgentListGoalRuns` | Show all runs |
| Get Goal Run | `AgentGetGoalRun` | Detail view |
| Control Goal Run | `AgentControlGoalRun` | pause/resume/cancel/rerun |

### 3.2 Goal Run States

```
queued → planning → running → [awaiting_approval] → completed
                  ↓           ↓
                paused      failed
                  ↓
                cancelled
```

### 3.3 Goal Run Structure

```rust
struct GoalRun {
    id: String,
    thread_id: String,
    goal: String,
    title: Option<String>,
    status: GoalRunStatus,
    steps: Vec<GoalRunStep>,
    current_step_index: usize,
    replan_budget: u32,
    generated_skill_path: Option<String>,
    reflection_summary: Option<String>,
    created_at: u64,
    completed_at: Option<u64>,
    last_error: Option<String>,
}

struct GoalRunStep {
    index: usize,
    title: String,
    status: GoalRunStepStatus,
    kind: GoalRunStepKind,
    execution_id: Option<String>,
    error: Option<String>,
}
```

### 3.4 TUI Views

- **Goal Run List**: Table with goal, status, progress, duration
- **Goal Run Detail**: Steps list with status indicators, current step highlight
- **Goal Run Controls**: Pause/Resume/Cancel/Rerun buttons
- **Step Inspector**: Expand step to see child execution entry, tool calls, output

---

## 4. Workspace Tasks And Execution Queue

Workspace tasks are operator-facing board cards whose targets are threads or goals. The daemon execution queue is lower-level runtime machinery. Some protocol names below may still use the legacy `AgentTask` term for queue records; docs should treat those as execution entries, not workspace tasks.

### 4.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Add Queue Entry | `AgentAddTask` | Queue background execution |
| List Queue Entries | `AgentListTasks` | Show execution queue entries |
| Cancel Queue Entry | `AgentCancelTask` | Remove from queue |

### 4.2 Queue Entry Structure

```rust
struct AgentTask {
    id: String,
    title: String,
    description: String,
    status: TaskStatus,     // legacy protocol name for queue-entry status
    priority: TaskPriority, // legacy protocol name for queue-entry priority
    command: Option<String>,
    session_id: Option<String>,
    scheduled_at: Option<u64>,
    dependencies: Vec<String>,
    created_at: u64,
    started_at: Option<u64>,
    completed_at: Option<u64>,
    exit_code: Option<i32>,
    logs: Vec<AgentTaskLogEntry>,
}

enum TaskStatus {
    Queued,
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    Completed,
    Failed,
    Cancelled,
}

enum TaskPriority {
    Low,
    Normal,
    High,
    Urgent,
}
```

### 4.3 TUI Views

- **Workspace Board**: Todo, In Progress, In Review, Done columns for workspace-owned tasks
- **Execution Queue Panel**: Priority-sorted queue entries with status icons
- **Queue Entry Detail**: Full description, dependencies, logs
- **Queue Scheduler**: Calendar/clock view for scheduled queue entries

---

## 5. Agent Threads

### 5.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Create Thread | `AgentCreateThread` | New conversation |
| List Threads | `AgentListThreads` | Show all threads |
| Get Thread | `AgentGetThread` | Full thread with messages |
| Delete Thread | `AgentDeleteThread` | Remove thread |
| Add Message | `AgentAddMessage` | Append to thread |
| List Messages | `AgentListMessages` | Get thread history |
| Subscribe | `AgentSubscribe` | Receive real-time events |

### 5.2 Thread Structure

```rust
struct AgentThread {
    id: String,
    title: Option<String>,
    created_at: u64,
    updated_at: u64,
    message_count: u64,
    total_tokens: u64,
    model: Option<String>,
    provider: Option<String>,
    goal_run_id: Option<String>,
}

struct AgentMessage {
    id: String,
    thread_id: String,
    created_at: i64,
    role: MessageRole,  // system, user, assistant
    content: String,
    provider: Option<String>,
    model: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning: Option<String>,
    tool_calls_json: Option<String>,
}
```

### 5.3 Agent Events (Real-time)

```rust
enum AgentEvent {
    Delta { thread_id, content },
    Reasoning { thread_id, content },
    ToolCall { thread_id, call_id, name, arguments },
    ToolResult { thread_id, call_id, name, content, is_error },
    Done { thread_id, input_tokens, output_tokens },
    Error { thread_id, message },
    TodoUpdated { thread_id, todos },
    GoalRunEvent { goal_run_id, phase, step_index },
    TaskEvent { task_id, status },
    HeartbeatTriggered { items },
}
```

### 5.4 TUI Views

- **Thread List**: Conversations with title, message count, last updated
- **Thread View**: Message history with role indicators
- **Streaming Output**: Real-time content delta rendering
- **Tool Call Panel**: Active tool calls with arguments/results
- **Reasoning Toggle**: Show/hide reasoning content

---

## 6. Memory Management

### 6.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Get Memory | `AgentGetMemory` | Retrieve SOUL/MEMORY/USER |
| Update Memory | `AgentUpdateMemory` | Persist memory content |

### 6.2 Memory Structure

```rust
struct AgentMemory {
    soul: String,         // SOUL.md - agent disposition
    memory: String,       // MEMORY.md - learned facts
    user_profile: String, // USER.md - operator preferences
}
```

### 6.3 TUI Views

- **Memory Panel**: Tabbed view for SOUL/MEMORY/USER
- **Memory Editor**: Text edit with save functionality
- **Memory Diff**: Show changes before committing

---

## 7. Skill Workflows

### 7.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Generate Skill | `GenerateSkill` | Create from history |
| List Snippets | `ListSnippets` | Show saved snippets |
| Create Snippet | `CreateSnippet` | Save new snippet |
| Run Snippet | `RunSnippet` | Execute in pane |

### 7.2 Skill Structure

```rust
struct Snippet {
    id: String,
    name: String,
    description: Option<String>,
    category: Option<String>,
    tags: Vec<String>,
    content: String,
    created_at: u64,
    owner: SnippetOwner,  // user or assistant
}
```

### 7.3 TUI Views

- **Skill Browser**: Categorized list with search
- **Skill Detail**: Full content, parameters, usage
- **Skill Generator**: Select history items to generate from

---

## 8. Workspace/Surface/Pane Management

### 8.1 Operations

| Operation | Tool/Message | Description |
|-----------|--------------|-------------|
| List Workspaces | `list_workspaces` | Show all workspaces |
| Create Workspace | `create_workspace` | New workspace |
| Set Active Workspace | `set_active_workspace` | Switch focus |
| Create Surface | `create_surface` | New tab in workspace |
| Set Active Surface | `set_active_surface` | Switch tab |
| Split Pane | `split_pane` | Horizontal/vertical split |
| Rename Pane | `rename_pane` | Custom name |
| Set Layout | `set_layout_preset` | Apply preset |
| Equalize Layout | `equalize_layout` | Balance splits |

### 8.2 Layout Presets

```
single      | 2-columns   | 3-columns
grid-2x2    | main-stack
```

### 8.3 Topology Structure

```rust
struct WorkspaceTopology {
    workspaces: Vec<WorkspaceTopologyEntry>,
}

struct WorkspaceTopologyEntry {
    workspace_id: String,
    workspace_name: String,
    surfaces: Vec<SurfaceTopologyEntry>,
}

struct SurfaceTopologyEntry {
    surface_id: String,
    surface_name: String,
    layout_mode: String,
    panes: Vec<PaneTopologyEntry>,
}

struct PaneTopologyEntry {
    pane_id: String,
    pane_name: Option<String>,
    session_id: Option<String>,
    type: String,  // terminal, browser
    url: Option<String>,
    title: Option<String>,
}
```

### 8.4 TUI Views

- **Workspace Switcher**: List/switch workspaces
- **Surface Tabs**: Tab bar for surfaces
- **Pane Focus**: Active pane indicator
- **Layout Controls**: Split, close, resize options

---

## 9. History & Search

### 9.1 Operations

| Operation | ClientMessage | Description |
|-----------|---------------|-------------|
| Search History | `SearchHistory` | FTS query |
| Append Command Log | `AppendCommandLog` | Log entry |
| Query Command Log | `QueryCommandLog` | Retrieve entries |
| List Transcript Index | `ListTranscriptIndex` | Saved transcripts |
| List Snapshot Index | `ListSnapshotIndex` | Checkpoints |

### 9.2 Data Structures

```rust
struct HistorySearchHit {
    id: String,
    timestamp: i64,
    session_id: Option<String>,
    command: String,
    exit_code: Option<i32>,
    cwd: Option<String>,
    preview: String,
}

struct CommandLogEntry {
    id: String,
    session_id: Option<String>,
    pane_id: Option<String>,
    workspace_id: Option<String>,
    command: String,
    cwd: Option<String>,
    started_at: i64,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
}

struct TranscriptIndexEntry {
    id: String,
    pane_id: Option<String>,
    workspace_id: Option<String>,
    filename: String,
    reason: Option<String>,
    captured_at: i64,
    size_bytes: Option<i64>,
    preview: Option<String>,
}
```

### 9.3 TUI Views

- **History Search**: Query input with results list
- **Command Log**: Filterable table with timestamps
- **Transcript Browser**: List with preview pane

---

## 10. Gateway Messaging

### 10.1 Operations

| Platform | Tool | Description |
|----------|------|-------------|
| Slack | `send_slack_message` | Post to channel |
| Discord | `send_discord_message` | Post to channel/user |
| Telegram | `send_telegram_message` | Send to chat |
| WhatsApp | `send_whatsapp_message` | Send to contact |

### 10.2 TUI Views

- **Gateway Status**: Connection indicators per platform
- **Message Composer**: Send via configured gateway

---

## 11. System Observability

### 11.1 Operations

| Operation | Tool | Description |
|-----------|------|-------------|
| System Info | `get_system_info` | CPU, memory, disk, load |
| Process List | `list_processes` | Top processes by CPU |
| Telemetry Integrity | `TelemetryIntegrity` | Verify WORM ledger |

### 11.2 TUI Views

- **System Panel**: Resource gauges
- **Process Table**: Sortable list with kill option

---

## Proposed TUI Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│ [W:Main] [S:Terminal] [S:Agent]                    [goal:fix-tests] │
├─────────────────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────┐ ┌─────────────────────────────┐ │
│ │                                 │ │ AGENT THREAD                │ │
│ │                                 │ │ ─────────────────────────── │ │
│ │       TERMINAL OUTPUT           │ │ User: Fix the failing tests │ │
│ │       (PTY Focus)               │ │                             │ │
│ │                                 │ │ Asst: I'll investigate...   │ │
│ │                                 │ │ ├─ ToolCall: search_files   │ │
│ │                                 │ │ │  └─ Result: 3 matches     │ │
│ │                                 │ │ ├─ ToolCall: read_file      │ │
│ │                                 │ │ │  └─ ...                   │ │
│ │                                 │ │                             │ │
│ └─────────────────────────────────┘ └─────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ GOAL: fix-tests                    │ WORKSPACE              │QUEUE │
│ ─────────────────────────────────  │ ─────────────────────  │────── │
│ [✓] Investigate failures           │ • analyze-logs (run)   │ 3    │
│ [▶] Fix the root cause             │ • run-tests (queued)   │      │
│ [ ] Verify fix                     │                        │      │
├─────────────────────────────────────────────────────────────────────┤
│ :command input ________________________________________ [APPROVE?]  │
└─────────────────────────────────────────────────────────────────────┘
```

### Layout Regions

| Region | Purpose | Navigation |
|--------|---------|------------|
| Header Bar | Workspaces, surfaces, active goal | `Tab`/`Shift+Tab` |
| Main Pane | Terminal output or agent thread | Focus follows surface |
| Goal Panel | Active goal steps | `g` to focus |
| Workspace Panel | Workspace tasks and execution queue | `t` to focus |
| Command Bar | Command input, approvals | `:` or `i` |

---

## Interaction Model

### Keyboard Shortcuts

```
Global:
  :         Command mode
  ?         Help overlay
  Tab       Next pane
  Shift+Tab Previous pane
  1-9       Jump to workspace N
  
Session:
  n         New session
  k         Kill session
  a         Attach to session
  
Agent:
  c         New thread
  Enter     Send message
  Esc       Cancel input
  
Goal:
  p         Pause goal
  r         Resume goal
  x         Cancel goal
  
Workspace:
  j/k       Navigate workspace tasks
  Enter     View task detail
  d         Soft-delete workspace task
```

### Command Palette

```
:help                  Show help
:sessions              List sessions
:goals                 List goal runs
:workspace             List workspace boards
:workspace-tasks       List workspace tasks
:queue                 List execution queue entries
:threads               List agent threads
:memory                Open memory editor
:skills                Browse skills
:history <query>       Search history
:approve <id>          Approve pending command
:reject <id>           Reject pending command
:workspace <name>      Switch workspace
:split <h|v>           Split pane
:layout <preset>       Apply layout preset
```

---

## Implementation Phases

### Phase 1: Core Infrastructure (P0)

1. **Session Management**
   - Session list view
   - PTY output rendering
   - Basic session lifecycle commands

2. **Command Execution**
   - Managed command queue
   - Approval modal
   - Command history

### Phase 2: Agent Integration (P1)

1. **Agent Threads**
   - Thread list and detail views
   - Message rendering with roles
   - Streaming delta handling

2. **Workspace Tasks And Execution Queue**
   - Workspace board list with status
   - Execution queue list with status
   - Queue scheduling
   - Dependency visualization

3. **Goal Runners**
   - Goal run list
   - Step progress view
   - Control commands (pause/resume/cancel)

### Phase 3: Extended Features (P2)

1. **Memory & Skills**
   - Memory panel with tabs
   - Skill browser
   - Skill generation

2. **History & Search**
   - FTS search
   - Command log viewer

3. **Workspace Management**
   - Workspace switcher
   - Layout presets
   - Pane navigation

### Phase 4: Polish (P2)

1. **System Observability**
2. **Gateway Messaging**
3. **Accessibility**
4. **Theme/Configuration**

---

## Technical Considerations

### TUI Framework Options

| Framework | Pros | Cons |
|-----------|------|------|
| **ratatui** | Mature, pure Rust, good docs | No async-native |
| **cursive** | Async support, flexible | Larger API surface |
| **tuirealm** | Elm-like architecture | Less mature |

**Recommendation**: `ratatui` with tokio for async integration.

### Daemon Communication

- Reuse existing `amux-protocol` codec
- Unix socket on Linux/macOS, named pipe on Windows
- Message framing via bincode

### State Management

```
TUI State
├── Session State (sessions, active_id)
├── Agent State (threads, active_thread_id)
├── Goal State (goal_runs, active_goal_id)
├── Workspace Task State (workspace tasks, filters)
├── Execution Queue State (queue entries, filters)
└── UI State (focus, layout, theme)
```

---

## Appendix: Full ClientMessage Reference

```rust
enum ClientMessage {
    // Session management
    Ping,
    SpawnSession { shell, cwd, env, workspace_id, cols, rows },
    CloneSession { source_id, workspace_id, cols, rows, cwd },
    KillSession { id },
    Resize { id, cols, rows },
    ListSessions,
    ListWorkspaceSessions { workspace_id },
    WriteInput { id, data },
    GetScrollback { id, max_lines },
    AnalyzeSession { id, max_lines },
    
    // Managed commands
    ExecuteManagedCommand { session, request },
    ResolveApproval { id, approval_id, decision },
    
    // History
    SearchHistory { query, limit },
    AppendCommandLog { entry_json },
    CompleteCommandLog { id, exit_code, duration_ms },
    QueryCommandLog { workspace_id, pane_id, limit },
    ClearCommandLog,
    
    // Agent persistence
    CreateAgentThread { thread_json },
    DeleteAgentThread { thread_id },
    ListAgentThreads,
    GetAgentThread { thread_id },
    AddAgentMessage { message_json },
    ListAgentMessages { thread_id, limit },
    
    // Transcripts & Snapshots
    UpsertTranscriptIndex { entry_json },
    ListTranscriptIndex { workspace_id },
    UpsertSnapshotIndex { entry_json },
    ListSnapshotIndex { workspace_id },
    ListSnapshots { workspace_id },
    SymbolSearch { query, workspace_root, limit },
    
    // Workspace topology
    PushWorkspaceTopology { topology },
    
    // Skills
    GenerateSkill { thread_id, max_messages, output_name },
    
    // Agent operations
    AgentCreateThread { title, session_id },
    AgentListThreads,
    AgentGetThread { thread_id },
    AgentDeleteThread { thread_id },
    AgentAddTask { title, description, priority, command, session_id, scheduled_at, dependencies },
    AgentStartGoalRun { goal, title, thread_id, session_id, priority },
    AgentCancelTask { task_id },
    AgentListTasks,
    AgentListGoalRuns,
    AgentGetGoalRun { goal_run_id },
    AgentControlGoalRun { goal_run_id, action, step_index },
    AgentListTodos,
    AgentGetTodos { thread_id },
    AgentGetConfig,
    AgentSetConfig { config_json },
    AgentHeartbeatGetItems,
    AgentHeartbeatSetItems { items_json },
    AgentSubscribe,
    AgentUnsubscribe,
    
    // Utilities
    ScrubText { text },
    GetGitStatus { path },
    VerifyTelemetryIntegrity,
    CheckpointSession { id, label },
}
```

---

## Appendix: Full DaemonMessage Reference

```rust
enum DaemonMessage {
    // Session events
    Pong,
    SessionSpawned { id, cols, rows, cwd, workspace_id },
    SessionExited { id, exit_code },
    Output { id, data },
    CommandStarted { id, command },
    CommandFinished { id, exit_code },
    CwdChanged { id, cwd },
    SessionList { sessions },
    Scrollback { id, data },
    
    // Managed command events
    ManagedCommandQueued { id, execution_id, position, snapshot },
    ApprovalRequired { id, approval },
    ApprovalResolved { id, approval_id, decision },
    ManagedCommandStarted { id, execution_id, command, source },
    ManagedCommandFinished { id, execution_id, command, exit_code, duration_ms, snapshot },
    ManagedCommandRejected { id, execution_id, message },
    
    // Analysis & Search
    AnalysisResult { id, result },
    HistorySearchResult { query, summary, hits },
    CommandLogRows { rows_json },
    
    // Agent events
    AgentEvent { event_json },
    AgentThreadList { threads_json },
    AgentThreadDetail { thread_json },
    AgentTaskList { tasks_json },
    AgentTaskEnqueued { task_json },
    AgentTaskCancelled { task_id, cancelled },
    AgentGoalRunStarted { goal_run_json },
    AgentGoalRunList { goal_runs_json },
    AgentGoalRunDetail { goal_run_json },
    AgentGoalRunControlled { goal_run_id, ok },
    AgentTodoList { todos_json },
    AgentTodoDetail { thread_id, todos_json },
    AgentConfigResponse { config_json },
    AgentHeartbeatItems { items_json },
    
    // Skills
    SkillGenerated { title, path },
    
    // Utilities
    ScrubResult { text },
    TelemetryIntegrityResult { results },
    SessionCheckpointed { id, ok, path, message },
    GitStatus { branch, is_dirty, ahead, behind, staged, modified, untracked },
    
    // Errors
    Error { message },
}
```

---

*End of TUI Capabilities Map*
