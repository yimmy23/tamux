# Self-Orchestrating Agent Architecture

tamux's daemon agent is a **self-orchestrating system** — it can autonomously decompose goals, manage sub-agents, monitor its own health, recover from failures, learn from execution patterns, and escalate when it gets stuck. This document describes the architecture in detail.

For the broader current-system view covering daemon ownership, memory layers, persistence, clients, and self-orchestrating capabilities, see [how-tamux-works.md](./how-tamux-works.md).

> **Quick orientation:** The agent module lives at `crates/amux-daemon/src/agent/` and is split across many focused files and submodules organized by responsibility. The entry point is `engine.rs` (struct definition) with the main execution loop in `agent_loop.rs`.

---

## Architecture Layers

The system is organized into four layers, each observing the one below it:

```
┌──────────────────────────────────────────────────────────────┐
│  Layer 3: Learning                                           │
│  traces · patterns · heuristics · effectiveness              │
├──────────────────────────────────────────────────────────────┤
│  Layer 2: Meta-Cognition                                     │
│  self_assessment · replanning · escalation · resource_alloc  │
├──────────────────────────────────────────────────────────────┤
│  Layer 1: Liveness & Monitoring                              │
│  checkpoints · health_monitor · stuck_detection · recovery   │
├──────────────────────────────────────────────────────────────┤
│  Layer 0: Execution                                          │
│  agent_loop · dispatcher · tool_executor · goal_planner      │
└──────────────────────────────────────────────────────────────┘
```

**Layer 0 (Execution)** handles LLM streaming, tool calls, task dispatch, and goal step progression. This is the hot path — every agent turn flows through here.

**Layer 1 (Liveness)** provides durability. Checkpoints snapshot goal state before and after each step. A health monitor detects degraded or stuck states. A recovery engine can restore from checkpoints after crashes.

**Layer 2 (Meta-Cognition)** adds self-awareness. The agent can assess its own progress, dynamically replan when stuck, escalate through 4 levels of intervention, and manage resource allocation across concurrent sub-agents.

**Layer 3 (Learning)** enables improvement over time. Execution traces record every tool call. Pattern recognition mines successful and failing sequences. Heuristics learned from past runs optimize future context allocation and tool selection.

---

## Module Map

### Core Execution (`agent/`)

| File | Lines | Purpose |
|------|-------|---------|
| `engine.rs` | ~130 | `AgentEngine` struct definition, constructor, constants |
| `engine_runtime.rs` | ~200 | Stream cancellation, repo watchers, memory cache |
| `agent_loop.rs` | ~810 | Core LLM+tool execution loop (`send_message_inner`) |
| `messaging.rs` | ~240 | Public message API, thread creation, session routing |
| `dispatcher.rs` | ~410 | Background execution and goal run dispatch scheduling |
| `task_crud.rs` | ~555 | Task/goal run CRUD operations (create, list, cancel, control) |
| `goal_planner.rs` | ~535 | Goal planning, step lifecycle, auto-checkpointing |
| `goal_llm.rs` | ~435 | Goal-specific LLM calls (plan, replan, reflect) |
| `goal_parsing.rs` | ~630 | Goal plan response parsing, JSON repair, utility helpers |
| `persistence.rs` | ~360 | State hydration from SQLite, all persist_* methods |

### Sub-Agent Management (`agent/subagent/`)

| File | Lines | Purpose |
|------|-------|---------|
| `tool_filter.rs` | ~300 | Whitelist/blacklist tool filtering with conflict detection |
| `context_budget.rs` | ~215 | Token budget tracking with overflow actions |
| `termination.rs` | ~715 | DSL parser for auto-stop conditions |
| `supervisor.rs` | ~520 | Health monitoring, stuck detection, intervention selection |
| `lifecycle.rs` | ~430 | State machine with transition validation |
| `tool_graph.rs` | ~635 | Tool relationships and composition caching |

### Liveness (`agent/liveness/`)

| File | Lines | Purpose |
|------|-------|---------|
| `state_layers.rs` | ~300 | 4-layer checkpoint model, health types |
| `checkpoint.rs` | ~785 | Save/load/list/prune goal state snapshots |
| `health_monitor.rs` | ~520 | Periodic assessment with hysteresis |
| `stuck_detection.rs` | ~785 | Generalized pattern-based stuck analysis |
| `recovery.rs` | ~365 | 3-strategy recovery planner |

### Context Management (`agent/context/`)

| File | Lines | Purpose |
|------|-------|---------|
| `context_item.rs` | ~215 | Typed, scored context items with relevance |
| `audit.rs` | ~375 | Categorize items as Critical/Active/Dormant/Archivable |
| `compression.rs` | ~585 | 3 strategies: Summarize, ExtractKeyPoints, SemanticCompress |
| `archive.rs` | ~465 | Store/search/retrieve evicted context with FTS5 |
| `restoration.rs` | ~555 | Retrieve archived context by query or thread |

### Meta-Cognitive Loop (`agent/metacognitive/`)

| File | Lines | Purpose |
|------|-------|---------|
| `self_assessment.rs` | ~450 | Progress/efficiency/quality metrics, momentum analysis |
| `replanning.rs` | ~615 | 6 re-planning strategies with automatic selection |
| `escalation.rs` | ~540 | 4-level graduated escalation pathway |
| `resource_alloc.rs` | ~445 | Dynamic context budgets, slot management, pressure monitoring |

### Learning (`agent/learning/`)

| File | Lines | Purpose |
|------|-------|---------|
| `traces.rs` | ~315 | Execution trace collection with SHA-256 argument hashing |
| `patterns.rs` | ~410 | Success/failure pattern mining with confidence scoring |
| `heuristics.rs` | ~415 | Learned context/tool/replan optimization |
| `effectiveness.rs` | ~560 | Per-tool and per-composition success tracking |

### Production Hardening (`agent/`)

| File | Lines | Purpose |
|------|-------|---------|
| `circuit_breaker.rs` | ~320 | 3-state circuit breaker for LLM API protection |
| `rate_limiter.rs` | ~300 | Per-tool token-bucket rate limiting |

---

## Key Concepts

### Sub-Agent Management

When work is too large for a single agent turn, the system spawns **sub-agents** — bounded child execution entries with fine-grained control:

```
Parent Work Item: "Refactor the auth module"
├── Sub-agent 1: "Extract middleware" [tool_whitelist: read_file, replace_in_file]
├── Sub-agent 2: "Write tests"        [context_budget: 20000 tokens]
└── Sub-agent 3: "Update docs"        [termination: timeout(300)]
```

Each sub-agent can have:
- **Tool filtering** — whitelist or blacklist specific tools
- **Context budget** — maximum tokens with overflow actions (Compress/Truncate/Error)
- **Termination conditions** — DSL expressions like `timeout(300) OR error_count(3)`
- **Supervisor config** — how often to check health and what intervention level to use
- **Max duration** — hard time limit

The tool filter operates at two levels: tools are removed from the LLM's schema (so it can't see them) AND blocked at execution time (defense in depth).

### Termination DSL

Sub-agents can be auto-stopped using a composable DSL:

```
timeout(300)                              # Stop after 5 minutes
tool_success_count(10)                    # Stop after 10 successful tool calls
timeout(600) OR error_count(3)            # Stop on timeout OR 3 errors
(timeout(300) AND tool_call_count(50)) OR error_count(5)  # Compound
NOT timeout(60)                           # Inverted (keep going if under 60s)
```

The DSL supports `AND`, `OR`, `NOT`, parenthetical grouping, and four condition types: `timeout(secs)`, `tool_success_count(n)`, `error_count(n)`, `tool_call_count(n)`.

### Checkpointing

Goal runs are automatically checkpointed:

- **Pre-step**: Before each goal run step is enqueued
- **Post-step**: After each step completes successfully
- **Manual**: User-triggered via protocol

Each checkpoint captures 4 layers of state:
1. **Goal State** — the GoalRun struct (plan, steps, status)
2. **Execution State** — active execution entries related to this goal
3. **Context State** — thread summaries and token counts
4. **Runtime State** — work context, TODOs, memory updates

Checkpoints are stored as JSON in the `agent_checkpoints` SQLite table with automatic pruning.

### Health Monitoring

The health monitor runs on a 30-second tick and uses **hysteresis** to prevent state flapping:

```
                    ┌─────────┐
          2 ticks   │ Degraded │  3 ticks
    ┌─────────────→ │          │ ──────────────┐
    │               └────┬─────┘               │
    │                    │                     ↓
┌───┴────┐         3 ticks back         ┌─────────┐
│ Healthy │ ←────────────────────────── │  Stuck   │
└────┬────┘                             └──────────┘
     │                                       │
     │        immediate (any state)          │
     └───────────────┬───────────────────────┘
                     ↓
              ┌──────────┐
              │  Crashed  │  (error_rate > 0.8 OR 5+ consecutive errors)
              └──────────┘
```

Health indicators tracked: `tool_call_frequency`, `error_rate`, `context_growth_rate`, `context_utilization_pct`, `consecutive_errors`.

### Stuck Detection

Five patterns are detected with confidence scoring:

| Pattern | Detection | Confidence |
|---------|-----------|------------|
| **NoProgress** | No tool calls for configured timeout | Scales with idle time |
| **ErrorLoop** | 3+ consecutive identical errors | Increases with error count |
| **ToolCallLoop** | A→B→A→B cycling pattern over 4+ calls | 0.85 fixed |
| **ResourceExhaustion** | Context utilization > 90% | Scales with overshoot |
| **Timeout** | Elapsed > max_duration_secs | Scales with overshoot |

The highest-confidence analysis is selected, and an intervention is chosen based on the stuck reason and configured intervention level (Passive/Normal/Aggressive).

### Escalation Pathway

When problems are detected, the system escalates through 4 levels:

| Level | Name | Action | Trigger |
|-------|------|--------|---------|
| L0 | Self-Correction | Auto-retry, context compress, strategy rotation | Default starting point |
| L1 | Sub-Agent | Spawn expert sub-agent for the stuck step | L0 fails 2x |
| L2 | User | Generate escalation message, wait for response | L1 fails |
| L3 | External | Notify via gateway (Slack/Discord), pause execution | L2 timeout |

Each level tracks attempts with automatic progression. The escalation state resets on success.

### Dynamic Re-Planning

When the assessment indicates problems, one of 6 strategies is selected:

1. **CompressRetry** — compress context, retry current step
2. **SpawnExpert** — spawn specialized sub-agent (expertise inferred from step title)
3. **UserGuidance** — request human input via approval workflow
4. **AlternativeTools** — disable the most-used tool to force a different approach
5. **Parallelize** — decompose stuck step into parallel sub-work
6. **GoalRevision** — trigger a full goal re-plan

Selection is based on the stuck reason, attempt count, error rate, and available resources.

### Resource Allocation

The resource pool manages concurrent sub-agent execution:

- **Slot management** — configurable max concurrent sub-agents (default: 3)
- **Token budgets** — allocated proportional to work complexity (Simple: 5K, Moderate: 15K, Complex: 30K, Research: 50K)
- **Priority preemption** — urgent execution entries can preempt low-priority slots
- **Pressure monitoring** — alerts when utilization exceeds 80%

### Context Self-Management

The context system maintains the agent's working memory:

- **Audit** — categorize all context items as Critical/Active/Dormant/Archivable based on recency, access frequency, and type
- **Compression** — 3 strategies automatically selected based on the compression ratio needed
- **Archival** — evicted context stored in SQLite with FTS5 full-text search
- **Restoration** — archived context retrieved by keyword query and injected back into the prompt

### Learning

The agent improves over time:

- **Execution traces** record every tool call with outcome, duration, and argument hashes
- **Pattern recognition** mines successful tool sequences and builds confidence scores
- **Heuristics** learn optimal context allocation and tool selection per task type
- **Effectiveness tracking** computes per-tool success rates and composition completion rates

Learned heuristics are injected into the system prompt as hints for future runs.

### Tool Composition Engine

Tools are modeled as a graph with typed relationships:

- **DependsOn** — `replace_in_file` depends on `read_file`
- **SynergizesWith** — `bash_command` + `read_file` work well together
- **ConflictsWith** — `bash_command` vs `execute_managed_command` (different execution models)

Successful tool sequences are cached with LRU eviction and can be promoted to permanent compositions after enough successful uses.

---

## Database Schema

Core orchestration state is stored in the daemon's `command-history.db` SQLite database. Some capability-specific operator artifacts intentionally live as sidecar files under the agent data directory, notably `operator_model.json` and the generated-tool registry/skill files:

| Table | Purpose |
|-------|---------|
| `subagent_metrics` | Per-sub-agent health metrics and tool call stats |
| `agent_checkpoints` | Goal run state snapshots for crash recovery |
| `agent_health_log` | Health state transitions and interventions |
| `context_archive` | Archived context with FTS5 full-text search |
| `execution_traces` | Tool call sequences and outcome metrics |
| `collaboration_sessions` | Persisted subagent collaboration-session snapshots |
| `memory_provenance` | Durable memory writes with confirmation/retraction metadata |
| `memory_provenance_relationships` | Explicit `retracts` links between memory provenance entries |

---

## Protocol Extensions

The IPC protocol includes these agent-specific messages:

| ClientMessage | DaemonMessage | Purpose |
|---------------|---------------|---------|
| `AgentGetSubagentMetrics` | `AgentSubagentMetrics` | Query sub-agent health |
| `AgentListCheckpoints` | `AgentCheckpointList` | List goal run checkpoints |
| `AgentRestoreCheckpoint` | `AgentCheckpointRestored` | Restore from checkpoint |
| `AgentGetHealthStatus` | `AgentHealthStatus` | System health overview |
| `AgentListHealthLog` | `AgentHealthLog` | Health state history |
| `AgentGetMemoryProvenanceReport` | `AgentMemoryProvenanceReport` | Query durable memory provenance state |
| `AgentConfirmMemoryProvenanceEntry` | `AgentMemoryProvenanceConfirmed` | Confirm a durable memory fact |
| `AgentRetractMemoryProvenanceEntry` | `AgentMemoryProvenanceRetracted` | Retract a durable memory fact |
| `AgentListGeneratedTools` | `AgentGeneratedTools` | Inspect generated-tool registry state |

Events broadcast to subscribed clients:

| Event | When |
|-------|------|
| `SubagentHealthChange` | Supervisor detects a state transition |
| `CheckpointCreated` | Auto-checkpoint before/after goal step |

---

## Configuration

Sub-agent features are configured per-task via `AgentTask` fields:

```json
{
  "tool_whitelist": ["read_file", "search_files"],
  "tool_blacklist": ["bash_command"],
  "context_budget_tokens": 20000,
  "context_overflow_action": "compress",
  "termination_conditions": "timeout(300) OR error_count(3)",
  "success_criteria": "All tests pass",
  "max_duration_secs": 600,
  "supervisor_config": {
    "check_interval_secs": 30,
    "stuck_timeout_secs": 300,
    "max_retries": 2,
    "intervention_level": "normal"
  }
}
```

All fields are optional with `#[serde(default)]` — existing execution records work unchanged.

---

## Test Coverage

The self-orchestrating agent system has broad unit coverage across orchestration, liveness, context, learning, and tool-composition modules.

Because the suite changes frequently, treat the repository test commands as the source of truth rather than relying on a fixed count in this document.
