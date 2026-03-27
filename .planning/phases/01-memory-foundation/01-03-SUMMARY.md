---
phase: 01-memory-foundation
plan: 03
subsystem: agent-intelligence
tags: [counter-who, negative-knowledge, self-model, constraint-graph, ttl-expiry, repeat-detection]

# Dependency graph
requires:
  - "01-01: Episodic memory data types, SQLite schema, episode CRUD, WORM ledger"
  - "01-02: Episodic retrieval, goal episode recording, system prompt injection"
provides:
  - "Counter-who self-model with repeat detection (compute_approach_hash, detect_repeated_approaches)"
  - "Counter-who operator correction tracking (record_correction, update_counter_who_on_correction)"
  - "Counter-who SQLite persistence and restore (persist_counter_who, restore_counter_who)"
  - "Counter-who context formatting for prompt injection (format_counter_who_context)"
  - "Negative knowledge constraint CRUD (add_negative_constraint, query_active_constraints)"
  - "Negative knowledge TTL expiry (expire_negative_constraints, is_constraint_active)"
  - "Negative knowledge prompt injection (format_negative_constraints with DO NOT attempt labels)"
  - "Auto-creation of negative constraints from failed goal episodes"
  - "Episode TTL expiry during consolidation (expire_old_episodes)"
affects: [goal-planning-quality, agent-loop-intelligence, consolidation-cleanup, system-prompt-context]

# Tech tracking
tech-stack:
  added: []
  patterns: [counter-who-self-model, negative-knowledge-constraint-graph, approach-hash-dedup, ttl-expiry-consolidation]

key-files:
  created:
    - crates/amux-daemon/src/agent/episodic/counter_who.rs
    - crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
  modified:
    - crates/amux-daemon/src/agent/episodic/mod.rs
    - crates/amux-daemon/src/agent/episodic/store.rs
    - crates/amux-daemon/src/agent/agent_loop.rs
    - crates/amux-daemon/src/agent/task_crud.rs
    - crates/amux-daemon/src/agent/system_prompt.rs
    - crates/amux-daemon/src/agent/consolidation.rs
    - crates/amux-daemon/src/agent/goal_llm.rs

key-decisions:
  - "Counter-who update wired into agent_loop.rs after tool execution provenance -- captures every tool result"
  - "Operator correction tracking wired into task_crud.rs approval denial -- only clear correction signal available"
  - "Negative knowledge injected into goal planning prompt (goal_llm.rs) rather than system prompt -- keeps system prompt lean, goal-specific constraints in goal path"
  - "Negative constraints also available via build_system_prompt parameter for future use by other callers"
  - "Episode and constraint expiry run as consolidation sub-tasks (budgeted, non-blocking)"
  - "FTS5 rebuild after episode expiry uses .ok() for tolerance of builds without FTS5"

patterns-established:
  - "Counter-who pattern: pure functions for logic + AgentEngine methods for state access + agent_loop wiring"
  - "Negative knowledge pattern: pure formatting functions + AgentEngine CRUD + goal planning injection"
  - "Consolidation extension pattern: add sub-task after existing tasks, guarded by deadline check"

requirements-completed: [CWHO-01, CWHO-02, CWHO-03, CWHO-04, NKNO-01, NKNO-02, NKNO-03, NKNO-04]

# Metrics
duration: 9min
tasks_completed: 2
tasks_total: 2
completed: 2026-03-27T01:33:19Z
---

# Phase 01 Plan 03: Counter-Who and Negative Knowledge Summary

Counter-who self-model with SHA-256 approach hashing, 3+ failure repeat detection, operator correction tracking, SQLite persistence/restore; negative knowledge constraint graph with entity-scoped queries, TTL expiry, DO NOT attempt prompt injection, and auto-creation from failed episodes.

## What Was Built

### Task 1: Counter-Who Self-Model

Created `counter_who.rs` with pure functions for approach hashing (`compute_approach_hash` via SHA-256 truncated to 16 chars), repeat detection (`detect_repeated_approaches` with configurable threshold), correction recording, old approach pruning (7-day max age, 20-entry cap), and context formatting for prompt injection.

Added `impl AgentEngine` methods: `update_counter_who_on_tool_result` (called after every tool execution in agent_loop.rs), `update_counter_who_on_correction` (called on approval denial in task_crud.rs), `persist_counter_who` (UPSERT to counter_who_state table), `restore_counter_who` (query by goal_run_id or "global").

Repeat detection fires `AgentEvent::CounterWhoAlert` when 3+ variants of the same approach hash fail. Persistent corrections fire alerts when the same pattern is corrected 2+ times.

### Task 2: Negative Knowledge Constraint Graph

Created `negative_knowledge.rs` with pure functions for constraint active checking (`is_constraint_active` via TTL) and formatting (`format_negative_constraints` with "DO NOT attempt" labels, solution class, confidence percentage, expiry countdown).

Added `impl AgentEngine` methods: `add_negative_constraint` (INSERT to negative_knowledge table + cache update), `record_negative_knowledge_from_episode` (auto-create from failed episodes with root cause), `query_active_constraints` (entity-scoped LIKE query, capped at 20), `expire_negative_constraints` (DELETE expired + cache cleanup), `refresh_constraint_cache`.

Added `negative_constraints` parameter to `build_system_prompt` (all callers updated). Wired active constraints into goal planning prompt in `goal_llm.rs`. Added constraint expiry and episode expiry as consolidation sub-tasks in `consolidation.rs`. Auto-creation of constraints from failed goal episodes wired into `record_goal_episode` in `store.rs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed rusqlite stmt lifetime issue in query_active_constraints**
- **Found during:** Task 2 implementation
- **Issue:** `stmt.query_map()` temporary borrows outlived the `stmt` binding when using `.collect()` pattern
- **Fix:** Replaced `.collect()` with explicit `for row in rows` loop to consume iterator before stmt drops
- **Files modified:** `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- **Commit:** 19756da

**2. [Rule 2 - Missing] Wired operator correction into task_crud.rs approval denial**
- **Found during:** Task 1 implementation
- **Issue:** Plan mentioned hooking into "approval rejection handling" in agent_loop.rs, but approval denial is actually handled in task_crud.rs
- **Fix:** Added counter-who correction tracking to the `ApprovalDecision::Deny` branch in task_crud.rs
- **Files modified:** `crates/amux-daemon/src/agent/task_crud.rs`
- **Commit:** 91d5fff

## Decisions Made

1. Counter-who update placed after provenance recording in agent_loop -- ensures every tool call is tracked regardless of outcome
2. Negative constraints injected into goal planning prompt (not system prompt) -- matches episodic context pattern from Plan 02
3. Consolidation expiry tasks run after skill lifecycle tasks but before learning store persistence

## Test Results

- 10 counter_who unit tests: all pass
- 7 negative_knowledge unit tests: all pass
- 42 total episodic module tests: all pass
- 918 total daemon tests: all pass (2 pre-existing plugin loader failures unrelated to changes)
- `cargo check -p tamux-daemon`: compiles with warnings only (all pre-existing)

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 | 91d5fff | Counter-who self-model with repeat detection and persistence |
| 2 | 19756da | Negative knowledge constraint graph with TTL and consolidation |

## Self-Check: PASSED

- All created files exist on disk
- All commit hashes found in git log
- cargo check passes
- All 42 episodic tests pass
