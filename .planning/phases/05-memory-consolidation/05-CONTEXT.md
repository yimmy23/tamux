# Phase 5: Memory Consolidation - Context

**Gathered:** 2026-03-23
**Status:** Ready for planning

<domain>
## Phase Boundary

The agent gets smarter during idle time — reviewing traces, aging stale facts, promoting heuristics, and maintaining continuity across restarts. This phase builds the consolidation engine that runs during detected idle periods, the fact decay/tombstone system for memory lifecycle, the heuristic promotion pipeline from execution traces to learned patterns, and seamless cross-session continuity after daemon restart.

Phase 5 does NOT add new memory storage formats (existing SOUL.md/MEMORY.md/USER.md + SQLite), does NOT change the memory flush mechanism (Phase 2's pre-compaction flush), and does NOT add community skill features (Phase 6/7).

</domain>

<decisions>
## Implementation Decisions

### Idle Consolidation Trigger
- **D-01:** Composite idle signal per MEMO-07: all must be true simultaneously — no active tasks + no active goal runs + no active streams + operator inactive >5min (from `AnticipatoryRuntime.last_presence_at`). Conservative — never consolidates during active work.
- **D-02:** Time-boxed 30-second budget per idle consolidation tick. Each tick processes traces/memory for max 30 seconds, then yields control back to the event loop. Multiple ticks accumulate progress over time. Prevents blocking the daemon.
- **D-03:** Consolidation runs as a heartbeat sub-phase: during `run_structured_heartbeat_adaptive()`, after check results are gathered but before LLM synthesis, check idle conditions. If idle, run consolidation tick. If not idle, skip. This piggybacks on the existing heartbeat schedule without adding a separate timer.

### Decay & Tombstone Model
- **D-04:** Exponential decay with lambda=0.01, ~69-hour half-life (per REQUIREMENTS.md blocker note). Facts have a `confidence` field (0.0-1.0) that decays exponentially from their `last_confirmed_at` timestamp. Configurable half-life via `AgentConfig.memory_decay_half_life_hours` (default: 69). Facts accessed or confirmed get their `last_confirmed_at` refreshed — active facts stay alive, unused facts fade.
- **D-05:** SQLite tombstone table for rollback: new `memory_tombstones` table preserving original content + metadata when a fact is superseded. Rollback = restore from tombstone + delete replacement. 7-day TTL on tombstone rows, auto-purged by consolidation. WORM provenance ledger records every write via existing `MemoryProvenanceRecord`.
- **D-06:** Append-only with tombstones per MEMO-03: consolidation never deletes MEMORY.md content directly. Superseded facts get a tombstone marker (`## [SUPERSEDED]` prefix) and are moved to the tombstone table. The MEMORY.md file only grows or has lines replaced — never shrinks. This ensures audit traceability.

### Heuristic Promotion Pipeline
- **D-07:** Success streak threshold: after N consecutive successes of the same tool pattern for the same task type (default N=3, configurable via `AgentConfig.heuristic_promotion_threshold`), the pattern is promoted to a learned heuristic. `HeuristicStore` already tracks `sample_count` — consolidation reviews traces, detects repeating patterns, bumps counts. `MIN_SAMPLES=5` in `heuristics.rs` gates when heuristics become "reliable."
- **D-08:** Hybrid heuristic influence: (1) System prompt injection — high-confidence heuristics (sample_count >= MIN_SAMPLES) are injected into the system prompt as a "Learned Patterns" section. E.g., "For file search tasks, prefer grep then read over recursive ls." Agent follows naturally via prompt conditioning. (2) Tool selection weighting — `ToolHeuristic.effectiveness_score` modulates tool ranking in `tool_executor.rs` for known task types. Both mechanisms active simultaneously.
- **D-09:** Consolidation reviews `ExecutionTrace` entries from `learning/traces.rs` during idle ticks. For each trace with `outcome: Success`, extract the tool sequence (list of `StepTrace.tool_name`), hash it, and check if this sequence has been seen before for this `task_type`. If the same sequence succeeds N times, promote to `ToolHeuristic`.

### Cross-Session Continuity
- **D-10:** Active context restoration on restart: the most recent active thread gets its context restored from FTS5 archive (using existing `RestorationRequest`). Agent's first message in the thread acknowledges continuity: "Resuming from where we left off — last working on [topic]." `HeuristicStore` and `OperatorModel` also restored from their persistence files.
- **D-11:** Interrupted goal runs marked Paused by default: goal runs in Running/Planning state when daemon stopped are marked `GoalRunStatus::Paused` on restart. User can resume explicitly. Configurable to auto-resume via `AgentConfig.auto_resume_goal_runs: bool` (default: false). Auto-resume waits for the first heartbeat after restart to confirm operator presence before resuming.
- **D-12:** Proactive memory refinement per MEMO-08: during consolidation ticks, scan MEMORY.md for redundant/contradictory facts (facts with overlapping keys but different values). When detected, use a short LLM call to merge/resolve, then update with tombstone for the original. Budget this within the 30-second tick.

### Claude's Discretion
- Exact idle detection polling interval (suggest: check at each heartbeat tick)
- Consolidation progress tracking (which traces have been reviewed)
- LLM prompt for memory refinement/merging
- "Learned Patterns" system prompt section format
- Tool effectiveness score formula in tool_executor.rs
- Context restoration depth (how many messages to restore)
- Continuity acknowledgment message template

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Memory System
- `crates/amux-daemon/src/agent/memory.rs` — 492 lines: SOUL.md/MEMORY.md/USER.md persistence, `MemoryTarget`, `MemoryUpdateMode`, `MemoryWriteContext`, fact deduplication, size limits
- `crates/amux-daemon/src/agent/memory_flush.rs` — Pre-compaction memory flush: `maybe_run_pre_compaction_memory_flush()`, saves facts before context compression
- `crates/amux-daemon/src/agent/provenance.rs` — 71 lines: WORM audit ledger integration, `MemoryProvenanceRecord`

### Learning System
- `crates/amux-daemon/src/agent/learning/heuristics.rs` — `HeuristicStore`, `ContextHeuristic`, `ToolHeuristic`, `ReplanHeuristic`, `update_context()`, `update_tool()`, `update_replan()`, `MIN_SAMPLES=5`
- `crates/amux-daemon/src/agent/learning/traces.rs` — `ExecutionTrace`, `StepTrace`, `CausalTrace`, `TraceOutcome`
- `crates/amux-daemon/src/agent/learning/patterns.rs` — 385 lines: Pattern mining from execution traces

### Context System
- `crates/amux-daemon/src/agent/context/archive.rs` — FTS5 archive for context items
- `crates/amux-daemon/src/agent/context/restoration.rs` — `RestorationRequest`, `RestoredItem` for context restoration
- `crates/amux-daemon/src/agent/persistence.rs` — 389 lines: `hydrate()` restores threads, tasks, memory, config on startup

### Heartbeat System (Phase 2-4)
- `crates/amux-daemon/src/agent/heartbeat.rs` — `run_structured_heartbeat_adaptive()` where consolidation sub-phase will integrate
- `crates/amux-daemon/src/agent/anticipatory.rs` — `AnticipatoryRuntime.last_presence_at` for operator presence tracking

### History Store
- `crates/amux-daemon/src/history.rs` — SQLite persistence layer. New tables (memory_tombstones, consolidation_log) will go here.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `HeuristicStore` with update methods — already tracks heuristic counts, just needs consolidation trigger
- `MemoryProvenanceRecord` — already records memory write provenance, usable for tombstone audit
- `RestorationRequest` + FTS5 archive — existing context restoration for cross-session continuity
- `hydrate()` in persistence.rs — existing startup restoration, needs extension for paused goal runs
- `AnticipatoryRuntime.last_presence_at` — existing presence tracking for idle detection
- `ExecutionTrace.steps` — existing per-step tool trace data for pattern extraction

### Established Patterns
- `AgentConfig` with `#[serde(default)]` for new config fields
- `HistoryStore` table creation in `ensure_tables_exist()` for new tables
- `tokio::task::spawn_blocking` for CPU-intensive work (pattern from heartbeat_checks.rs)
- `run_structured_heartbeat_adaptive()` sub-phase pattern from Phase 4

### Integration Points
- `run_structured_heartbeat_adaptive()` — add consolidation sub-phase after check results
- `system_prompt.rs` — inject learned heuristics section
- `tool_executor.rs` — modulate tool selection with effectiveness scores
- `persistence.rs hydrate()` — add goal run pausing and context restoration
- `history.rs` — new memory_tombstones and consolidation_log tables

</code_context>

<specifics>
## Specific Ideas

- Consolidation should feel invisible — the agent just "knows more" over time without the user noticing the mechanism
- The 30-second budget prevents consolidation from ever being noticeable as a performance impact
- Facts that are actively used get refreshed, creating a natural "spaced repetition" effect — the agent remembers what matters
- Goal run pausing on restart is safer than auto-resume — the user should never be surprised by the agent continuing work they may have forgotten about
- Tombstone table with 7-day TTL is a safety net, not a feature — if the user never needs rollback, the tombstones quietly expire

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 05-memory-consolidation*
*Context gathered: 2026-03-23*
