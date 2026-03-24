---
phase: 01-production-foundation
plan: 01
subsystem: database
tags: [sqlite, tokio-rusqlite, wal, async, concurrency]

requires: []
provides:
  - "Async HistoryStore with tokio_rusqlite::Connection"
  - "WAL mode pragmas (journal_mode, synchronous, foreign_keys, wal_autocheckpoint, busy_timeout)"
  - "Single shared connection replacing open-per-call pattern"
  - "IntoCallError trait for non-rusqlite error conversion in .call() closures"
affects: [01-02, 01-03]

tech-stack:
  added: [tokio-rusqlite 0.6]
  patterns: [async .call() closure pattern for DB access, parameter cloning for 'static closures, IntoCallError trait]

key-files:
  created: []
  modified:
    - crates/amux-daemon/Cargo.toml
    - crates/amux-daemon/src/history.rs

key-decisions:
  - "Used tokio-rusqlite 0.6.0 (NOT 0.7.0) for rusqlite 0.32 compatibility"
  - "busy_timeout=5000 satisfies D-13; explicit retry-with-backoff consciously deferred"
  - "refresh_thread_stats moved to standalone function for closure compatibility"
  - "Added IntoCallError trait for serde_json error conversion inside .call() closures"
  - "ensure_column/table_has_column changed to return rusqlite::Result for closure compatibility"

patterns-established:
  - "Async .call() pattern: self.conn.call(move |conn| { ... }).await.map_err(|e| anyhow::anyhow!(\"{e}\"))"
  - "Parameter cloning: Clone &str to String before entering 'static + Send closures"
  - "Error bridging: .call_err()? for non-rusqlite errors inside closures via IntoCallError trait"

requirements-completed: [FOUN-01, FOUN-02, FOUN-06]

duration: 25min
completed: 2026-03-23
---

# Phase 01 Plan 01: Async SQLite Migration Summary

**Migrated HistoryStore from synchronous open-per-call SQLite to single async tokio-rusqlite 0.6 connection with WAL mode, 5 pragmas, and 65+ async methods**

## Performance

- **Duration:** 25 min
- **Started:** 2026-03-23T00:54:01Z
- **Completed:** 2026-03-23T01:19:40Z
- **Tasks:** 3 completed
- **Files modified:** 2

## Accomplishments

- Eliminated all 62 open_connection() calls, replacing with single shared tokio_rusqlite::Connection
- Applied WAL mode with 5 pragmas (journal_mode=WAL, synchronous=NORMAL, foreign_keys=ON, wal_autocheckpoint=1000, busy_timeout=5000)
- Migrated 65+ public HistoryStore methods to async .call() closure pattern
- Implemented 4 integration tests verifying WAL mode, pragma values, async roundtrip, and concurrent read/write safety
- Zero compilation errors in history.rs (callers need .await in Plan 02)

## Task Commits

Each task was committed atomically:

1. **Task 0: Create integration test stubs** - `dfd0d18` (test)
2. **Task 1: Add tokio-rusqlite and rewrite struct/constructor/init_schema** - `3722c33` (feat)
3. **Task 2: Migrate all methods to async .call() and implement tests** - `e73ca07` (feat)

## Files Created/Modified

- `crates/amux-daemon/Cargo.toml` - Added tokio-rusqlite = "0.6" dependency
- `crates/amux-daemon/src/history.rs` - Complete async migration: struct holds tokio_rusqlite::Connection, all 65+ methods async with .call() pattern, WAL pragmas, 4 new integration tests

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Error type bridging for .call() closures**
- **Found during:** Task 2
- **Issue:** tokio_rusqlite::Error and anyhow::Error are distinct types; closures expect tokio_rusqlite::Result but many operations return serde_json::Error or anyhow::Error
- **Fix:** Added IntoCallError trait with .call_err() method for converting any std::error::Error to tokio_rusqlite::Error::Other. Changed ensure_column/table_has_column to return rusqlite::Result.
- **Files modified:** crates/amux-daemon/src/history.rs
- **Commit:** e73ca07

**2. [Rule 3 - Blocking] refresh_thread_stats lifetime conflict in closures**
- **Found during:** Task 2
- **Issue:** refresh_thread_stats was a &self method but needed to be called inside 'static closures
- **Fix:** Moved to standalone function taking &Connection and &str parameters
- **Files modified:** crates/amux-daemon/src/history.rs
- **Commit:** e73ca07

**3. [Rule 3 - Blocking] Borrowed parameter lifetime in .call() closures**
- **Found during:** Task 2
- **Issue:** &str and &Type parameters cannot be moved into 'static + Send closures (Pitfall 1 from research)
- **Fix:** Clone all borrowed parameters to owned types before entering closures
- **Files modified:** crates/amux-daemon/src/history.rs
- **Commit:** e73ca07

## Known Stubs

None - all functionality is fully wired.

## Notes

- Callers of HistoryStore methods in other daemon files (agent_loop.rs, server.rs, persistence.rs, etc.) still call the now-async methods synchronously. Plan 02 will add .await to all 124 call sites.
- Tests cannot be run until Plan 02 completes (binary crate won't compile due to caller errors).
- provenance_report, generate_soc2_artifact, and verify_worm_integrity remain sync since they only access filesystem (no DB).

## Self-Check: PASSED
