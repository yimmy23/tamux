---
phase: 01-production-foundation
plan: 02
subsystem: database
tags: [sqlite, arc, shared-state, broadcast-channel, async, concurrency]

requires:
  - "Async HistoryStore with tokio_rusqlite::Connection (from Plan 01)"
provides:
  - "Single shared Arc<HistoryStore> created in server.rs and distributed to all components"
  - "Zero independent HistoryStore::new() calls outside server.rs"
  - "Configurable broadcast channel capacities via AgentConfig (PTY=1024, agent events=512)"
  - "SessionManager::new_with_history() accepting Arc<HistoryStore> and pty_channel_capacity"
  - "AgentEngine::new_with_shared_history() accepting Arc<HistoryStore>"
  - "load_config_from_history() replacing load_config()"
  - "PTY thread async history bridging via Handle::block_on()"
affects: [01-03]

tech-stack:
  added: []
  patterns: [Arc<HistoryStore> shared ownership, Handle::block_on() for sync-to-async bridge in PTY threads, configurable channel capacity via serde defaults]

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/server.rs
    - crates/amux-daemon/src/session_manager.rs
    - crates/amux-daemon/src/snapshot.rs
    - crates/amux-daemon/src/agent/engine.rs
    - crates/amux-daemon/src/agent/task_prompt.rs
    - crates/amux-daemon/src/agent/types.rs
    - crates/amux-daemon/src/pty_session.rs
    - crates/amux-daemon/src/agent/persistence.rs
    - crates/amux-daemon/src/agent/causal_traces.rs
    - crates/amux-daemon/src/agent/tool_executor.rs

key-decisions:
  - "Used Arc<HistoryStore> at SessionManager level, clone inner HistoryStore for AgentEngine (HistoryStore is Clone-cheap via tokio_rusqlite::Connection handle)"
  - "Used Handle::block_on() in PTY read thread for async history calls (3 call sites: record_managed_finish, detect_skill_candidates, generate_skill)"
  - "Made all SessionManager wrapper methods async to propagate .await through server.rs"
  - "Made snapshot free functions (enforce_retention, delete_snapshot, etc.) async"
  - "PTY channel default 1024, agent event channel default 512 (up from hardcoded 256)"

patterns-established:
  - "Shared HistoryStore: server.rs creates Arc<HistoryStore>, passes to SessionManager and AgentEngine"
  - "Sync-to-async bridge: capture Handle::current() before std::thread::spawn, use block_on() inside"
  - "Configurable channel capacity: serde(default = 'fn') pattern for backward-compatible config fields"

requirements-completed: [FOUN-03, FOUN-05]

duration: 36min
completed: 2026-03-23
---

# Phase 01 Plan 02: Shared HistoryStore Wiring and Configurable Channel Capacity Summary

**Wired single Arc<HistoryStore> through all 4 daemon components, added .await to 120+ call sites across 29 files, made broadcast channel capacities configurable (PTY=1024, agent events=512)**

## Performance

- **Duration:** 36 min
- **Started:** 2026-03-23T01:23:10Z
- **Completed:** 2026-03-23T01:59:23Z
- **Tasks:** 2 completed
- **Files modified:** 30

## Accomplishments

- Eliminated 3 independent HistoryStore::new() calls (session_manager.rs, engine.rs, snapshot.rs) plus 1 ad-hoc call in task_prompt.rs
- Single Arc<HistoryStore> created in server.rs and shared via Arc to SessionManager, AgentEngine, and SnapshotStore
- Added .await to 120+ async HistoryStore method calls across 29 files
- Made broadcast channel capacities configurable via AgentConfig with backward-compatible serde defaults
- All 503 daemon tests pass with zero compilation errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire shared Arc<HistoryStore> through all components** - `0bcb2e2` (feat)
2. **Task 2: Add configurable broadcast channel capacity** - `d3368d1` (feat)

## Files Created/Modified

- `crates/amux-daemon/src/server.rs` - Creates shared HistoryStore, passes to SessionManager and AgentEngine
- `crates/amux-daemon/src/session_manager.rs` - new_with_history(Arc<HistoryStore>, usize), Arc<HistoryStore> field, all wrapper methods async
- `crates/amux-daemon/src/snapshot.rs` - new_with_history(HistoryStore), all methods async
- `crates/amux-daemon/src/agent/engine.rs` - new_with_shared_history(), configurable agent_event_channel_capacity
- `crates/amux-daemon/src/agent/task_prompt.rs` - load_config_from_history(&HistoryStore)
- `crates/amux-daemon/src/agent/types.rs` - pty_channel_capacity and agent_event_channel_capacity fields
- `crates/amux-daemon/src/pty_session.rs` - pty_channel_capacity param, Handle::block_on() for async bridge
- `crates/amux-daemon/src/agent/persistence.rs` - All history calls .await, persist_thread_snapshot async
- `crates/amux-daemon/src/agent/causal_traces.rs` - All insert_causal_trace calls .await, build_causal_guidance_summary async
- 20 additional agent module files updated with .await on history method calls

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed pre-existing test compilation from Plan 01**
- **Found during:** Task 2
- **Issue:** Tests in history.rs, messaging.rs, config.rs, semantic_env.rs, tool_executor.rs used old sync SessionManager::new() and AgentEngine::new_test() APIs
- **Fix:** Updated test constructors to use async SessionManager::new_test() and AgentEngine::new_test().await, added .await to history calls in tests, fixed provenance_report (sync not async) test call
- **Files modified:** history.rs, messaging.rs, config.rs, semantic_env.rs, tool_executor.rs
- **Commit:** d3368d1

**2. [Rule 2 - Missing Critical] Added SessionManager::new_test() constructor for tests**
- **Found during:** Task 2
- **Issue:** Removing SessionManager::new() left tests with no way to construct a SessionManager
- **Fix:** Added #[cfg(test)] pub async fn new_test(root: &Path) -> Arc<Self> that creates a test HistoryStore
- **Files modified:** session_manager.rs
- **Commit:** d3368d1

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical)
**Impact on plan:** Both fixes necessary for test compilation. No scope creep.

## Known Stubs

None - all functionality is fully wired.

## Notes

- The `verify_worm_integrity()` method remains sync in SessionManager since it only reads filesystem (WORM ledger files), not the database
- `provenance_report()` and `generate_soc2_artifact()` also remain sync as they read telemetry JSONL files
- Plan 03 (heartbeat split) can proceed -- all async infrastructure is in place

## Self-Check: PASSED
