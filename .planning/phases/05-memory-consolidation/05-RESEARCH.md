# Phase 5: Memory Consolidation - Research

**Researched:** 2026-03-23
**Domain:** Agent memory lifecycle, idle consolidation, heuristic learning, cross-session continuity
**Confidence:** HIGH

## Summary

Phase 5 builds four interconnected subsystems on top of the existing daemon infrastructure: (1) an idle-triggered consolidation engine that piggybacks on the heartbeat cycle, (2) a fact decay and tombstone system for memory lifecycle management, (3) a heuristic promotion pipeline that converts execution trace patterns into learned heuristics, and (4) cross-session continuity that gracefully restores context after daemon restarts.

The existing codebase provides excellent foundations. `HeuristicStore`, `PatternStore`, `EffectivenessTracker`, and `ExecutionTrace` already exist in `learning/` with full test suites. `MemoryProvenanceRecord` and `record_memory_provenance()` already log every memory write. `RestorationRequest` and FTS5 archive provide context restoration. `AnticipatoryRuntime.last_presence_at` tracks operator presence. The heartbeat function `run_structured_heartbeat_adaptive()` has a clear phase structure (Phases 0-9) where a consolidation sub-phase can be inserted. The key engineering challenge is wiring these existing building blocks into a coherent consolidation pipeline that runs within a 30-second time budget during idle heartbeat ticks.

**Primary recommendation:** Build consolidation as a new module `consolidation.rs` in `crates/amux-daemon/src/agent/` with a single entry point `maybe_run_consolidation_tick()` called from within `run_structured_heartbeat_adaptive()` as a new phase between the current Phase 9 (weight update) and the final log. Keep each consolidation sub-task (trace review, decay, refinement, tombstone cleanup) as independent functions that share a time budget, enabling incremental progress across multiple heartbeat ticks.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Composite idle signal per MEMO-07: all must be true simultaneously -- no active tasks + no active goal runs + no active streams + operator inactive >5min (from `AnticipatoryRuntime.last_presence_at`). Conservative -- never consolidates during active work.
- **D-02:** Time-boxed 30-second budget per idle consolidation tick. Each tick processes traces/memory for max 30 seconds, then yields control back to the event loop. Multiple ticks accumulate progress over time. Prevents blocking the daemon.
- **D-03:** Consolidation runs as a heartbeat sub-phase: during `run_structured_heartbeat_adaptive()`, after check results are gathered but before LLM synthesis, check idle conditions. If idle, run consolidation tick. If not idle, skip. This piggybacks on the existing heartbeat schedule without adding a separate timer.
- **D-04:** Exponential decay with lambda=0.01, ~69-hour half-life (per REQUIREMENTS.md blocker note). Facts have a `confidence` field (0.0-1.0) that decays exponentially from their `last_confirmed_at` timestamp. Configurable half-life via `AgentConfig.memory_decay_half_life_hours` (default: 69). Facts accessed or confirmed get their `last_confirmed_at` refreshed -- active facts stay alive, unused facts fade.
- **D-05:** SQLite tombstone table for rollback: new `memory_tombstones` table preserving original content + metadata when a fact is superseded. Rollback = restore from tombstone + delete replacement. 7-day TTL on tombstone rows, auto-purged by consolidation. WORM provenance ledger records every write via existing `MemoryProvenanceRecord`.
- **D-06:** Append-only with tombstones per MEMO-03: consolidation never deletes MEMORY.md content directly. Superseded facts get a tombstone marker (`## [SUPERSEDED]` prefix) and are moved to the tombstone table. The MEMORY.md file only grows or has lines replaced -- never shrinks. This ensures audit traceability.
- **D-07:** Success streak threshold: after N consecutive successes of the same tool pattern for the same task type (default N=3, configurable via `AgentConfig.heuristic_promotion_threshold`), the pattern is promoted to a learned heuristic. `HeuristicStore` already tracks `sample_count` -- consolidation reviews traces, detects repeating patterns, bumps counts. `MIN_SAMPLES=5` in `heuristics.rs` gates when heuristics become "reliable."
- **D-08:** Hybrid heuristic influence: (1) System prompt injection -- high-confidence heuristics (sample_count >= MIN_SAMPLES) are injected into the system prompt as a "Learned Patterns" section. (2) Tool selection weighting -- `ToolHeuristic.effectiveness_score` modulates tool ranking in `tool_executor.rs` for known task types. Both mechanisms active simultaneously.
- **D-09:** Consolidation reviews `ExecutionTrace` entries from `learning/traces.rs` during idle ticks. For each trace with `outcome: Success`, extract the tool sequence (list of `StepTrace.tool_name`), hash it, and check if this sequence has been seen before for this `task_type`. If the same sequence succeeds N times, promote to `ToolHeuristic`.
- **D-10:** Active context restoration on restart: the most recent active thread gets its context restored from FTS5 archive (using existing `RestorationRequest`). Agent's first message in the thread acknowledges continuity: "Resuming from where we left off -- last working on [topic]." `HeuristicStore` and `OperatorModel` also restored from their persistence files.
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

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| MEMO-01 | During idle heartbeat ticks, agent reviews recent execution traces and consolidates learnings into MEMORY.md | Consolidation engine as heartbeat sub-phase (D-03); `list_execution_traces()` already queries traces from SQLite; `apply_memory_update()` writes to MEMORY.md with provenance |
| MEMO-02 | Memory facts have confidence scores that decay exponentially over time (configurable half-life, default ~69 hours) | New `MemoryFact` struct with `confidence` and `last_confirmed_at` fields; exponential decay formula `e^(-lambda * age_hours)` with lambda=0.01 (D-04); stored in SQLite `memory_facts` table |
| MEMO-03 | Consolidation is append-only with tombstones -- never deletes, only marks facts as superseded | New `memory_tombstones` SQLite table (D-05); `[SUPERSEDED]` markers in MEMORY.md (D-06); existing `MemoryProvenanceRecord` for audit |
| MEMO-04 | All consolidation actions logged to provenance system with full audit trail | Existing `record_provenance_event()` in `provenance.rs` and `record_memory_provenance()` in `memory.rs` already provide the audit mechanism |
| MEMO-05 | 7-day rollback window: any consolidation can be reversed within 7 days | Tombstone table with `created_at` timestamp; rollback restores from tombstone; auto-purge tombstones older than 7 days during consolidation ticks (D-05) |
| MEMO-06 | Successful tool sequences automatically promoted into learned heuristics during consolidation | `ExecutionTrace.steps` provides tool sequences; `PatternStore.record_sequence()` tracks patterns; threshold at N=3 consecutive successes (D-07); `HeuristicStore.update_tool()` promotes to heuristic |
| MEMO-07 | Idle detection uses composite signal: no active tasks + no active goals + no active streams + operator inactive | Check `self.tasks` for any InProgress, `self.goal_runs` for any Running/Planning, `self.stream_cancellations` for active streams, `AnticipatoryRuntime.last_presence_at` for >5min idle (D-01) |
| MEMO-08 | Proactive memory refinement: reorganize and compress memory blocks for higher signal density during idle time | LLM call within 30-second tick budget to merge/resolve contradictory/redundant facts detected by `extract_memory_fact_candidates()` (D-12) |
| MEMO-09 | Cross-session context continuity: threads resume seamlessly after daemon restart with full context | Extend `hydrate()` in `persistence.rs` to pause interrupted goal runs (D-11), restore most recent thread context from FTS5 archive (D-10), restore HeuristicStore/OperatorModel |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.32 (bundled) | SQLite for tombstones, consolidation_log, memory_facts tables | Already the project's persistence layer via `HistoryStore` |
| tokio-rusqlite | 0.6.0 | Async SQLite access | Already used project-wide per Phase 1 |
| serde + serde_json | 1.x | Serialization for consolidation state and configs | Already used everywhere |
| sha2 | 0.10 | Hash tool sequences for dedup | Already a project dependency |
| tracing | 0.1 | Structured logging for consolidation events | Already the project's logging framework |
| anyhow | 1.x | Error handling | Already the project standard |
| uuid | 1.x | Generate IDs for tombstones and consolidation entries | Already a project dependency |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| reqwest | 0.12 | HTTP client for LLM calls during memory refinement (D-12) | Only during MEMO-08 refinement sub-task |
| humantime | 2.x | Human-readable formatting for decay/age displays | For logging and debug output |

**No new dependencies needed.** All required libraries are already in the workspace. This phase is entirely internal to the daemon crate.

## Architecture Patterns

### Recommended Project Structure

```
crates/amux-daemon/src/agent/
  consolidation.rs           # NEW: Main consolidation engine module
  consolidation/             # ALTERNATIVE: If module grows large, extract to directory
    mod.rs                   #   Entry point, idle detection, tick orchestration
    decay.rs                 #   Fact confidence decay logic
    tombstone.rs             #   Tombstone create/restore/cleanup
    trace_review.rs          #   Execution trace -> heuristic promotion
    refinement.rs            #   LLM-powered memory dedup/merge
  memory.rs                  # MODIFY: Add MemoryFact with confidence, last_confirmed_at
  heartbeat.rs               # MODIFY: Add Phase 9.5 consolidation sub-phase call
  persistence.rs             # MODIFY: Extend hydrate() for goal run pausing + context restoration
  system_prompt.rs           # MODIFY: Add "Learned Patterns" section injection
  learning/heuristics.rs     # MODIFY: Add build_learned_patterns_prompt() method
  types.rs                   # MODIFY: Add consolidation config fields to AgentConfig
  engine.rs                  # MODIFY: Add consolidation state fields if needed
```

### Pattern 1: Time-Budgeted Consolidation Tick

**What:** A single consolidation tick runs multiple sub-tasks (trace review, decay, refinement, tombstone cleanup) within a shared 30-second wall-clock budget. Each sub-task checks remaining time before starting. Sub-tasks yield if budget is exhausted.

**When to use:** Every heartbeat tick when idle conditions are met.

**Example:**
```rust
// Source: Project-specific pattern derived from heartbeat phase structure
pub(super) async fn maybe_run_consolidation_tick(
    &self,
    budget: std::time::Duration,
) -> ConsolidationResult {
    let deadline = std::time::Instant::now() + budget;
    let mut result = ConsolidationResult::default();

    // Sub-task 1: Review execution traces -> promote heuristics
    if std::time::Instant::now() < deadline {
        result.traces_reviewed = self.review_execution_traces(&deadline).await;
    }

    // Sub-task 2: Decay stale memory facts
    if std::time::Instant::now() < deadline {
        result.facts_decayed = self.decay_memory_facts(&deadline).await;
    }

    // Sub-task 3: Cleanup expired tombstones (7-day TTL)
    if std::time::Instant::now() < deadline {
        result.tombstones_purged = self.cleanup_expired_tombstones().await;
    }

    // Sub-task 4: Proactive memory refinement (LLM call, most expensive)
    if std::time::Instant::now() < deadline {
        result.facts_refined = self.refine_memory_facts(&deadline).await;
    }

    result
}
```

### Pattern 2: Idle Detection as Pure Function

**What:** Idle detection is extracted as a testable pure function that takes current state as inputs and returns a boolean. This follows the established `check_quiet_window()` and `should_broadcast()` pattern from heartbeat.rs.

**When to use:** At the start of each consolidation attempt.

**Example:**
```rust
// Source: Following heartbeat.rs pure function pattern (check_quiet_window, should_broadcast)
pub(super) fn is_idle_for_consolidation(
    active_task_count: usize,
    active_goal_run_count: usize,
    active_stream_count: usize,
    last_presence_at: Option<u64>,
    now: u64,
    idle_threshold_ms: u64,
) -> bool {
    if active_task_count > 0 || active_goal_run_count > 0 || active_stream_count > 0 {
        return false;
    }
    match last_presence_at {
        Some(last) => now.saturating_sub(last) >= idle_threshold_ms,
        None => true, // No presence recorded = consider idle
    }
}
```

### Pattern 3: Exponential Decay with Refresh

**What:** Memory facts have a `confidence` field that decays exponentially from `last_confirmed_at`. When a fact is accessed or used, `last_confirmed_at` is refreshed, resetting the decay clock. This creates a natural "spaced repetition" effect.

**When to use:** During fact decay sub-task of consolidation tick.

**Example:**
```rust
// Source: Mathematical formula from D-04 context decision
const DEFAULT_HALF_LIFE_HOURS: f64 = 69.0;

fn compute_fact_confidence(
    last_confirmed_at: u64,
    now: u64,
    half_life_hours: f64,
) -> f64 {
    let lambda = (2.0_f64.ln()) / half_life_hours;
    let age_hours = (now.saturating_sub(last_confirmed_at) as f64) / 3_600_000.0;
    (-lambda * age_hours).exp()
}
```

### Pattern 4: Heartbeat Sub-Phase Integration

**What:** Consolidation is a new phase in the heartbeat pipeline, inserted after Phase 9 (weight update) as Phase 9.5 or Phase 10. It checks idle conditions first, then runs the consolidation tick.

**When to use:** Every `run_structured_heartbeat_adaptive()` call.

**Integration point in heartbeat.rs:**
```rust
// After Phase 9 (weight update loop) at line ~791 in heartbeat.rs:

// --- Phase 10: Memory consolidation (MEMO-01 through MEMO-08) ---
let consolidation_result = self.maybe_run_consolidation_if_idle(
    std::time::Duration::from_secs(30),
).await;
if let Some(result) = consolidation_result {
    tracing::info!(
        traces = result.traces_reviewed,
        decayed = result.facts_decayed,
        tombstones = result.tombstones_purged,
        refined = result.facts_refined,
        "consolidation tick completed"
    );
}
```

### Pattern 5: Tombstone Table with TTL Purge

**What:** A `memory_tombstones` SQLite table stores superseded fact content with full metadata. During each consolidation tick, rows older than 7 days are purged. Rollback is a simple operation: restore tombstone content, delete replacement.

**Example schema:**
```sql
CREATE TABLE IF NOT EXISTS memory_tombstones (
    id                TEXT PRIMARY KEY,
    target            TEXT NOT NULL,         -- 'MEMORY.md', 'SOUL.md', 'USER.md'
    original_content  TEXT NOT NULL,
    fact_key          TEXT,
    replaced_by       TEXT,                  -- Content that superseded this fact
    replaced_at       INTEGER NOT NULL,
    source_kind       TEXT NOT NULL,         -- 'consolidation', 'tool', 'goal_reflection'
    provenance_id     TEXT,                  -- Links to memory_provenance record
    created_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tombstones_created ON memory_tombstones(created_at);
CREATE INDEX IF NOT EXISTS idx_tombstones_target ON memory_tombstones(target, created_at DESC);
```

### Anti-Patterns to Avoid
- **Blocking the event loop:** Never run consolidation synchronously for unbounded time. The 30-second budget with deadline checking prevents this.
- **Direct MEMORY.md deletion:** Per D-06, never shrink MEMORY.md. Always use tombstone + supersede pattern.
- **Consolidation during active work:** Per D-01, the composite idle signal must be checked. Missing even one condition (e.g., forgetting active streams) could cause consolidation during operator interaction.
- **Unbounded LLM calls in refinement:** The memory refinement sub-task (D-12) uses an LLM call. This must be budgeted within the 30-second tick. Use a short system prompt and small context window. If the circuit breaker is open, skip refinement.
- **Losing tombstones on crash:** Tombstone writes should happen before the MEMORY.md update, not after. If the daemon crashes between creating the tombstone and updating MEMORY.md, no data is lost -- the tombstone simply refers to content that was never actually superseded.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tool sequence hashing | Custom hash function | `sha2::Sha256` via existing `hash_arguments()` in `traces.rs` | Already implemented, consistent with codebase |
| Pattern detection | New pattern mining | `PatternStore.record_sequence()` in `learning/patterns.rs` | Already implements occurrence tracking, confidence, and decay |
| Tool effectiveness tracking | New tracking system | `EffectivenessTracker` in `learning/effectiveness.rs` | Already tracks per-tool and per-composition stats |
| Heuristic storage | New data store | `HeuristicStore` in `learning/heuristics.rs` | Already has `update_tool()`, `update_context()`, `update_replan()`, `build_system_prompt_hints()` |
| Memory fact key extraction | New parser | `extract_memory_fact_candidates()` and `derive_fact_key()` in `memory.rs` | Already handles key:value, "X is Y", "X uses Y" patterns |
| Context restoration | New restoration system | `RestorationRequest` + `rank_and_select()` in `context/restoration.rs` | Already implements FTS5 search, relevance ranking, token budgets |
| Audit trail | Custom logging | `record_provenance_event()` + `record_memory_provenance()` | Already integrated with WORM ledger |
| LLM call infrastructure | Direct HTTP | `send_completion_request()` or `self.send_message()` | Already handles streaming, circuit breakers, retries, transport selection |

**Key insight:** Phase 5 is primarily a *wiring* phase -- connecting existing building blocks rather than creating new fundamental capabilities. The learning, pattern, effectiveness, heuristic, memory, and restoration systems already exist. The new code is the consolidation orchestrator, the decay math, the tombstone table, and the heartbeat integration.

## Common Pitfalls

### Pitfall 1: Consolidation Running During Active Work
**What goes wrong:** Consolidation triggers while the operator is actively using the agent, causing unexpected LLM calls, memory updates, or performance impacts.
**Why it happens:** Incomplete idle detection -- checking only some signals (e.g., no active tasks) but missing others (e.g., active streams, operator typing).
**How to avoid:** The idle check must be a composite of ALL four signals from D-01. Implement as a pure function with explicit parameters for testability. Write tests for every combination of active/idle states.
**Warning signs:** Consolidation log entries appearing during active conversation threads.

### Pitfall 2: Tombstone-Before-Update Ordering
**What goes wrong:** MEMORY.md is updated before the tombstone is written. If the daemon crashes between the update and the tombstone write, the original content is lost with no rollback path.
**Why it happens:** Natural coding order puts the "update" first and the "record" second.
**How to avoid:** Always write the tombstone to SQLite first, then update MEMORY.md, then write the provenance record. SQLite is durable (WAL mode); the file system update to MEMORY.md is the more fragile operation.
**Warning signs:** Memory provenance records without corresponding tombstone entries.

### Pitfall 3: LLM Budget Exhaustion in Refinement
**What goes wrong:** The memory refinement LLM call (D-12) takes longer than the remaining 30-second budget, blocking the heartbeat return.
**Why it happens:** LLM calls can be unpredictable in duration, especially with slow providers or large contexts.
**How to avoid:** Run refinement as the LAST sub-task. Check remaining budget before starting. Use a short, focused prompt with minimal context (just the contradictory facts, not the entire MEMORY.md). Apply a timeout to the LLM call itself. If the circuit breaker is open, skip refinement entirely.
**Warning signs:** Heartbeat cycle durations exceeding 45 seconds when consolidation is active.

### Pitfall 4: Decay Formula Producing NaN or Infinity
**What goes wrong:** Edge cases in the decay formula produce NaN (0/0) or infinity (extremely large age values), corrupting fact confidence scores.
**Why it happens:** `last_confirmed_at` of 0 (never confirmed), clock skew producing negative ages, or integer overflow in timestamp math.
**How to avoid:** Use `saturating_sub` for all timestamp arithmetic. Clamp confidence to `0.0..=1.0` after computation. Handle the `last_confirmed_at == 0` case explicitly (treat as "confidence = 0.0"). Add unit tests for edge cases including timestamps at 0, `u64::MAX`, and negative deltas.
**Warning signs:** Facts with `NaN` or negative confidence values in the memory_facts table.

### Pitfall 5: Heuristic Promotion Count Reset on Restart
**What goes wrong:** Execution trace review progress is lost on daemon restart, causing the same traces to be re-reviewed and heuristic counts to be double-counted.
**Why it happens:** If the "last reviewed trace ID" or "last reviewed timestamp" is only kept in memory and not persisted.
**How to avoid:** Persist the consolidation progress watermark (last reviewed trace `created_at` timestamp) in a `consolidation_log` SQLite table. On startup, resume from the watermark.
**Warning signs:** Heuristic `sample_count` values jumping by large amounts after daemon restarts.

### Pitfall 6: Goal Run State Race on Restart
**What goes wrong:** Goal runs that were Running when the daemon stopped are auto-resumed before the operator has a chance to review what happened, potentially continuing work on a stale or unwanted goal.
**Why it happens:** Auto-resume without waiting for operator presence confirmation.
**How to avoid:** Per D-11, default `auto_resume_goal_runs` to `false`. Mark interrupted goal runs as `Paused` during `hydrate()`. If auto-resume is enabled, wait for the first heartbeat tick (which confirms operator presence) before resuming.
**Warning signs:** Goal runs transitioning from Paused to Running without operator interaction.

### Pitfall 7: Memory Size Limits During Consolidation Writes
**What goes wrong:** Consolidation tries to append learned facts to MEMORY.md but hits the `MEMORY_LIMIT_CHARS` (2200) limit, causing the write to fail.
**Why it happens:** MEMORY.md is already near capacity; consolidation adds more content without checking available space.
**How to avoid:** Before any consolidation write, check `content.chars().count()` against `MemoryTarget::Memory.limit_chars()`. If near capacity, prioritize refinement (merge/compress existing facts) over appending new facts. Use the existing `validate_memory_size()` check.
**Warning signs:** Consolidation log entries showing "memory limit exceeded" errors.

## Code Examples

### Idle Detection (Pure Function)
```rust
// Source: Derived from D-01 composite idle signal requirements
/// Check all four idle conditions required for consolidation.
/// Returns true only when ALL conditions are simultaneously met.
pub(super) fn is_idle_for_consolidation(
    active_task_count: usize,
    running_goal_count: usize,
    active_stream_count: usize,
    last_presence_at: Option<u64>,
    now: u64,
    idle_threshold_ms: u64, // default: 5 * 60 * 1000 (5 minutes)
) -> bool {
    if active_task_count > 0 {
        return false;
    }
    if running_goal_count > 0 {
        return false;
    }
    if active_stream_count > 0 {
        return false;
    }
    match last_presence_at {
        Some(last) => now.saturating_sub(last) >= idle_threshold_ms,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_when_all_conditions_met() {
        assert!(is_idle_for_consolidation(0, 0, 0, Some(1000), 400_000, 300_000));
    }

    #[test]
    fn not_idle_with_active_task() {
        assert!(!is_idle_for_consolidation(1, 0, 0, Some(1000), 400_000, 300_000));
    }

    #[test]
    fn not_idle_with_recent_presence() {
        assert!(!is_idle_for_consolidation(0, 0, 0, Some(350_000), 400_000, 300_000));
    }
}
```

### Exponential Decay
```rust
// Source: D-04 decay formula with lambda = ln(2) / half_life_hours
/// Compute current confidence for a memory fact based on exponential decay.
///
/// Returns a value in `0.0..=1.0`. A fact confirmed exactly `half_life_hours`
/// ago will have confidence ~0.5. Active facts (recently confirmed) stay near 1.0.
pub fn compute_decay_confidence(
    last_confirmed_at: u64,
    now: u64,
    half_life_hours: f64,
) -> f64 {
    if last_confirmed_at == 0 || half_life_hours <= 0.0 {
        return 0.0;
    }
    let age_ms = now.saturating_sub(last_confirmed_at) as f64;
    let age_hours = age_ms / 3_600_000.0;
    let lambda = 2.0_f64.ln() / half_life_hours;
    let confidence = (-lambda * age_hours).exp();
    confidence.clamp(0.0, 1.0)
}
```

### Tombstone Write-Before-Update Pattern
```rust
// Source: Derived from D-05 tombstone requirements and Pitfall 2 prevention
async fn supersede_memory_fact(
    &self,
    target: MemoryTarget,
    original_content: &str,
    fact_key: &str,
    replacement_content: &str,
) -> Result<()> {
    // Step 1: Write tombstone FIRST (durable)
    let tombstone_id = format!("tomb_{}", Uuid::new_v4());
    self.history.insert_memory_tombstone(
        &tombstone_id,
        target.label(),
        original_content,
        fact_key,
        replacement_content,
        now_millis(),
    ).await?;

    // Step 2: Update MEMORY.md (may fail without data loss)
    let agent_data_dir = &self.data_dir;
    apply_memory_update(
        agent_data_dir,
        &self.history,
        target,
        MemoryUpdateMode::Replace,
        replacement_content,
        MemoryWriteContext {
            source_kind: "consolidation",
            thread_id: None,
            task_id: None,
            goal_run_id: None,
        },
    ).await?;

    // Step 3: Record provenance (audit trail)
    self.record_provenance_event(
        "memory_consolidation",
        &format!("Superseded fact '{}' in {}", fact_key, target.label()),
        serde_json::json!({
            "tombstone_id": tombstone_id,
            "fact_key": fact_key,
        }),
        None, None, None, None, None,
    ).await;

    Ok(())
}
```

### Heuristic Promotion from Traces
```rust
// Source: Derived from D-07/D-09 using existing PatternStore and HeuristicStore
async fn review_execution_traces(
    &self,
    deadline: &std::time::Instant,
) -> usize {
    let watermark = self.get_consolidation_watermark("trace_review").await;
    let traces = match self.history.list_recent_successful_traces(
        watermark,
        50, // batch size
    ).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("failed to load execution traces for consolidation: {e}");
            return 0;
        }
    };

    let mut reviewed = 0;
    let promotion_threshold = self.config.read().await
        .heuristic_promotion_threshold
        .unwrap_or(3);

    for trace in &traces {
        if std::time::Instant::now() >= *deadline {
            break;
        }

        let tool_seq: Vec<String> = trace.steps.iter()
            .map(|s| s.tool_name.clone())
            .collect();

        if tool_seq.is_empty() {
            reviewed += 1;
            continue;
        }

        // Record in PatternStore (tracks occurrences)
        // Check if pattern crosses promotion threshold
        // If so, update HeuristicStore with new tool heuristics

        reviewed += 1;
    }

    // Update watermark
    if let Some(last) = traces.last() {
        self.set_consolidation_watermark("trace_review", last.created_at).await;
    }

    reviewed
}
```

### Goal Run Pausing on Restart (persistence.rs extension)
```rust
// Source: D-11 -- extend hydrate() to pause interrupted goal runs
// In persistence.rs, after loading goal_runs:
match self.history.list_goal_runs().await {
    Ok(goal_runs) if !goal_runs.is_empty() => {
        let mut runs: VecDeque<GoalRun> = goal_runs.into_iter().collect();
        // D-11: Mark interrupted goal runs as Paused
        for goal_run in runs.iter_mut() {
            if matches!(goal_run.status, GoalRunStatus::Running | GoalRunStatus::Planning) {
                goal_run.status = GoalRunStatus::Paused;
                goal_run.events.push(GoalRunEvent {
                    timestamp: now_millis(),
                    kind: "paused_on_restart".to_string(),
                    message: "Daemon restarted; goal run paused for operator review.".to_string(),
                });
            }
        }
        *self.goal_runs.lock().await = runs;
        self.persist_goal_runs().await;
    }
    // ... existing fallback paths
}
```

### Learned Patterns System Prompt Section
```rust
// Source: D-08 -- inject heuristics into system prompt
// In system_prompt.rs, add after the Skills section:
fn build_learned_patterns_section(heuristic_store: &HeuristicStore) -> String {
    let mut section = String::new();

    // Only include heuristics with enough samples (MIN_SAMPLES = 5)
    let reliable_tools: Vec<&ToolHeuristic> = heuristic_store.tool_heuristics.iter()
        .filter(|h| h.usage_count >= 5 && h.effectiveness_score >= 0.6)
        .collect();

    if reliable_tools.is_empty() {
        return section;
    }

    section.push_str("\n\n## Learned Patterns\n");
    section.push_str("These patterns were learned from successful past executions:\n");

    // Group by task_type
    let mut by_task: HashMap<&str, Vec<&ToolHeuristic>> = HashMap::new();
    for h in &reliable_tools {
        by_task.entry(&h.task_type).or_default().push(h);
    }

    for (task_type, tools) in &by_task {
        section.push_str(&format!("\n### For '{}' tasks:\n", task_type));
        for tool in tools {
            section.push_str(&format!(
                "- Prefer `{}` ({:.0}% effective, {} uses)\n",
                tool.tool_name,
                tool.effectiveness_score * 100.0,
                tool.usage_count,
            ));
        }
    }

    section
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No memory lifecycle | Exponential decay with configurable half-life | Phase 5 | Facts naturally age out; active knowledge stays fresh |
| Direct memory deletion | Append-only with tombstones | Phase 5 | Full audit trail; 7-day rollback safety net |
| Manual heuristic configuration | Auto-promotion from execution traces | Phase 5 | Agent autonomously learns effective tool patterns |
| No cross-session continuity | FTS5 archive restoration + paused goal runs | Phase 5 | Seamless experience across daemon restarts |
| Static system prompt | Dynamic "Learned Patterns" section from heuristics | Phase 5 | Agent behavior adapts to operator's project patterns |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `#[tokio::test]` |
| Config file | None needed -- Rust's built-in test runner |
| Quick run command | `cargo test -p amux-daemon -- agent::consolidation` |
| Full suite command | `cargo test -p amux-daemon` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MEMO-01 | Idle tick reviews traces and updates memory | integration | `cargo test -p amux-daemon -- agent::consolidation::tests::consolidation_tick_reviews_traces -x` | Wave 0 |
| MEMO-02 | Fact confidence decays with half-life | unit | `cargo test -p amux-daemon -- agent::consolidation::tests::decay_confidence -x` | Wave 0 |
| MEMO-03 | Append-only with tombstone markers | unit | `cargo test -p amux-daemon -- agent::consolidation::tests::tombstone_preserves_original -x` | Wave 0 |
| MEMO-04 | All actions logged to provenance | integration | `cargo test -p amux-daemon -- agent::consolidation::tests::provenance_recorded -x` | Wave 0 |
| MEMO-05 | Rollback within 7 days works | unit | `cargo test -p amux-daemon -- agent::consolidation::tests::rollback_restores_tombstone -x` | Wave 0 |
| MEMO-06 | Tool sequences promoted to heuristics | unit | `cargo test -p amux-daemon -- agent::consolidation::tests::trace_promotes_heuristic -x` | Wave 0 |
| MEMO-07 | Composite idle detection | unit | `cargo test -p amux-daemon -- agent::consolidation::tests::idle_detection -x` | Wave 0 |
| MEMO-08 | Memory refinement merges contradictions | integration | `cargo test -p amux-daemon -- agent::consolidation::tests::refinement_merges_facts -x` | Wave 0 |
| MEMO-09 | Cross-session continuity after restart | integration | `cargo test -p amux-daemon -- agent::persistence::tests::hydrate_pauses_interrupted_goals -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p amux-daemon -- agent::consolidation --lib -x`
- **Per wave merge:** `cargo test -p amux-daemon`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/amux-daemon/src/agent/consolidation.rs` -- new module, all consolidation tests
- [ ] New `memory_tombstones` and `consolidation_log` tables in `history.rs` schema
- [ ] New `list_recent_successful_traces()` query variant in `history.rs` for watermark-based retrieval
- [ ] New `MemoryFact` struct with confidence/decay fields

## Open Questions

1. **HeuristicStore and PatternStore persistence location**
   - What we know: Both are defined as serializable types with `#[derive(Serialize, Deserialize)]` but are not visible as fields on `AgentEngine`. They may be ephemeral (rebuilt on each run) or persisted elsewhere.
   - What's unclear: Whether they are currently persisted to disk/SQLite, or only exist in-memory during a session. The `agent_loop.rs` creates `TraceCollector` instances and inserts traces to SQLite, but the stores themselves may not be loaded on startup.
   - Recommendation: Add `HeuristicStore` and `PatternStore` as `RwLock`-guarded fields on `AgentEngine`. Persist them to JSON files (like `heartbeat.json`) or SQLite. Load them during `hydrate()`. This is essential for consolidation to accumulate knowledge across sessions.

2. **Consolidation progress watermark storage**
   - What we know: A `consolidation_log` table is mentioned in CONTEXT.md canonical references. The watermark (last reviewed trace timestamp) needs to survive restarts.
   - What's unclear: Whether to use a dedicated SQLite table or a simpler key-value approach.
   - Recommendation: Use a simple `consolidation_state` SQLite table with `key TEXT PRIMARY KEY, value TEXT, updated_at INTEGER`. Store watermarks as key-value pairs (e.g., `trace_review_watermark` -> `1711234567890`). This is more flexible than a structured log table and cheaper.

3. **Memory refinement LLM model selection**
   - What we know: D-12 requires an LLM call to merge contradictory facts. The daemon supports multiple providers and models.
   - What's unclear: Whether to use the operator's configured primary model or a cheaper/faster model for refinement. Using the primary model consumes tokens from the operator's quota.
   - Recommendation: Use the operator's configured model (via existing `send_message()` or `send_completion_request()`). The refinement context is small (just the contradictory facts, ~200 tokens), so cost is minimal. Circuit breaker integration prevents calls when the provider is down.

## Project Constraints (from CLAUDE.md)

- **Tech stack:** Rust daemon only -- no frontend changes needed for Phase 5. All consolidation logic is daemon-internal.
- **Local-first:** All consolidation data stays on the operator's machine. No cloud calls except LLM API for memory refinement (using operator's own API key).
- **Backward compatibility:** New SQLite tables are additive (created in `ensure_tables_exist()`). New `AgentConfig` fields use `#[serde(default)]`. Existing MEMORY.md format is preserved -- tombstone markers are additive annotations.
- **Error handling:** Use `anyhow` for all consolidation errors. Log failures via `tracing::warn!()` and continue -- consolidation failures should never crash the daemon or block the heartbeat.
- **Naming conventions:** Module and function names follow project `snake_case` convention. Types follow `PascalCase` convention. Constants follow `UPPER_SNAKE_CASE`.
- **Test patterns:** Builder helpers use `make_` prefix. Factory helpers use `sample_` prefix. Inline `#[test]` and `#[tokio::test]` in module files.
- **GSD Workflow:** All changes must go through GSD workflow per CLAUDE.md enforcement.

## Sources

### Primary (HIGH confidence)
- `crates/amux-daemon/src/agent/memory.rs` -- Full review of memory persistence, fact extraction, contradiction detection, provenance recording
- `crates/amux-daemon/src/agent/learning/heuristics.rs` -- Full review of HeuristicStore, ToolHeuristic, MIN_SAMPLES, update methods, system prompt hints
- `crates/amux-daemon/src/agent/learning/traces.rs` -- Full review of ExecutionTrace, StepTrace, TraceCollector, TraceOutcome, tool sequence extraction
- `crates/amux-daemon/src/agent/learning/patterns.rs` -- Full review of PatternStore, ToolPattern, record_sequence, confidence computation, decay
- `crates/amux-daemon/src/agent/learning/effectiveness.rs` -- Full review of EffectivenessTracker, ToolStats, CompositionStats
- `crates/amux-daemon/src/agent/heartbeat.rs` -- Full review of structured heartbeat phases 0-9, integration points
- `crates/amux-daemon/src/agent/anticipatory.rs` -- Full review of AnticipatoryRuntime, last_presence_at, idle detection signals
- `crates/amux-daemon/src/agent/persistence.rs` -- Full review of hydrate(), persist methods, task/goal state management
- `crates/amux-daemon/src/agent/context/restoration.rs` -- Full review of RestorationRequest, RestoredItem, rank_and_select, build_restoration_context
- `crates/amux-daemon/src/agent/provenance.rs` -- Full review of record_provenance_event()
- `crates/amux-daemon/src/agent/memory_flush.rs` -- Full review of pre-compaction flush pattern (reusable LLM call pattern)
- `crates/amux-daemon/src/agent/system_prompt.rs` -- Review of build_system_prompt structure and injection points
- `crates/amux-daemon/src/agent/engine.rs` -- Review of AgentEngine fields and constructor
- `crates/amux-daemon/src/agent/types.rs` -- Review of AgentConfig, GoalRunStatus, GoalRun structures
- `crates/amux-daemon/src/history.rs` -- Review of execution_traces table schema, insert/list methods, memory_provenance table

### Secondary (MEDIUM confidence)
- `.planning/phases/05-memory-consolidation/05-CONTEXT.md` -- All decisions D-01 through D-12 and canonical references

### Tertiary (LOW confidence)
- None -- all findings verified against codebase source code

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, no new dependencies needed
- Architecture: HIGH -- integration points verified by reading actual source code of heartbeat, memory, learning, and persistence modules
- Pitfalls: HIGH -- derived from actual code patterns observed (ordering dependencies, limit checks, state persistence)
- Code examples: HIGH -- based on actual function signatures, types, and patterns from the codebase

**Research date:** 2026-03-23
**Valid until:** 2026-04-22 (30 days -- stable internal codebase, no external dependency changes)
