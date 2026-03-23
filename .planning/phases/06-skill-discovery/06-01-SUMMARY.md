---
phase: 06-skill-discovery
plan: 01
subsystem: agent
tags: [skill-discovery, maturity-lifecycle, jaccard, novelty-detection, sqlite]

# Dependency graph
requires:
  - phase: 05-memory-consolidation
    provides: ConsolidationResult, ExecutionTraceRow, PatternStore, consolidation pipeline
provides:
  - SkillMaturityStatus enum (Draft/Testing/Active/Proven/PromotedToCanonical)
  - SkillDiscoveryConfig and SkillPromotionConfig on AgentConfig
  - Pure functions for complexity scoring, novelty detection, Jaccard similarity
  - update_skill_variant_status() and get_skill_variant() DB methods
  - Preflight filter excludes draft-status skills
affects: [06-02-PLAN, 06-03-PLAN, skill-evolution, heartbeat, consolidation]

# Tech tracking
tech-stack:
  added: []
  patterns: [pure-function-for-testability, serde-default-config-pattern, status-enum-with-as_str]

key-files:
  created:
    - crates/amux-daemon/src/agent/skill_discovery.rs
  modified:
    - crates/amux-daemon/src/agent/types.rs
    - crates/amux-daemon/src/history.rs
    - crates/amux-daemon/src/agent/skill_preflight.rs
    - crates/amux-daemon/src/agent/heartbeat.rs
    - crates/amux-daemon/src/agent/mod.rs

key-decisions:
  - "SkillMaturityStatus supports legacy kebab-case parsing (promoted-to-canonical) for backward compat"
  - "Pure functions take pre-fetched patterns (not PatternStore directly) for testability"
  - "Two empty sets yield Jaccard similarity 1.0 (mathematically correct convention)"

patterns-established:
  - "Status enum with as_str/from_status_str pattern for DB string roundtripping"
  - "Config structs with serde default functions following SkillDiscoveryConfig pattern"

requirements-completed: [SKIL-01, SKIL-02, SKIL-03, SKIL-05]

# Metrics
duration: 6min
completed: 2026-03-23
---

# Phase 06 Plan 01: Skill Discovery Foundation Summary

**SkillMaturityStatus lifecycle enum, complexity/novelty pure functions, DB status methods, and preflight draft exclusion with 28 passing tests**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-23T17:16:08Z
- **Completed:** 2026-03-23T17:22:08Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- SkillMaturityStatus enum with Draft/Testing/Active/Proven/PromotedToCanonical stages and bidirectional string conversion
- SkillDiscoveryConfig and SkillPromotionConfig structs with serde defaults on AgentConfig
- skill_discovery.rs module with meets_complexity_threshold, jaccard_similarity, is_novel_sequence, and extract_tool_sequence_from_json
- update_skill_variant_status() and get_skill_variant() added to HistoryStore for lifecycle transitions
- Preflight filter updated to exclude draft-status skills from agent context
- 28 new tests all passing, 654 total daemon tests with zero regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Foundation types, config extensions, DB methods, preflight** - `da6bc5a` (feat)
2. **Task 2: skill_discovery.rs module with pure functions** - `d7a71fa` (feat)

_Both tasks followed TDD: tests written alongside implementation, all passing._

## Files Created/Modified
- `crates/amux-daemon/src/agent/skill_discovery.rs` - New module with complexity scoring, Jaccard similarity, novelty detection, JSON extraction
- `crates/amux-daemon/src/agent/types.rs` - SkillMaturityStatus enum, SkillDiscoveryConfig, SkillPromotionConfig, ConsolidationResult skill fields, HeartbeatCheckType::SkillLifecycle
- `crates/amux-daemon/src/history.rs` - update_skill_variant_status() and get_skill_variant() async DB methods
- `crates/amux-daemon/src/agent/skill_preflight.rs` - Added "draft" to excluded status filter
- `crates/amux-daemon/src/agent/heartbeat.rs` - Added SkillLifecycle arm to check_type_to_action_type match
- `crates/amux-daemon/src/agent/mod.rs` - Registered skill_discovery module

## Decisions Made
- SkillMaturityStatus supports legacy kebab-case parsing ("promoted-to-canonical") for backward compatibility with any existing data
- Pure functions accept pre-fetched patterns slice rather than PatternStore reference for easier unit testing
- Two empty sets yield Jaccard similarity 1.0, following the mathematical convention
- HeartbeatCheckType::SkillLifecycle added proactively to avoid breaking match exhaustiveness (deviation Rule 3)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added SkillLifecycle arm to heartbeat.rs match block**
- **Found during:** Task 1 (HeartbeatCheckType variant addition)
- **Issue:** Adding SkillLifecycle to HeartbeatCheckType enum caused non-exhaustive match in check_type_to_action_type()
- **Fix:** Added `HeartbeatCheckType::SkillLifecycle => "skill_lifecycle"` arm
- **Files modified:** crates/amux-daemon/src/agent/heartbeat.rs
- **Verification:** cargo test -p tamux-daemon passes (654 tests, 0 failures)
- **Committed in:** da6bc5a (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Essential for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All foundation types and pure functions ready for Plan 02 (consolidation pipeline integration)
- SkillDiscoveryConfig and SkillPromotionConfig accessible via AgentConfig for runtime configuration
- update_skill_variant_status() ready for lifecycle state machine in Plan 03
- Preflight correctly excludes draft skills so testing/active/proven skills flow through

---
*Phase: 06-skill-discovery*
*Completed: 2026-03-23*
