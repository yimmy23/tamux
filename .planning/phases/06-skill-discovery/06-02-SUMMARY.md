---
phase: 06-skill-discovery
plan: 02
subsystem: agent
tags: [skill-discovery, consolidation, lifecycle, mental-tests, hybrid-llm, worm-provenance]

# Dependency graph
requires:
  - phase: 06-skill-discovery
    plan: 01
    provides: SkillMaturityStatus, SkillDiscoveryConfig, SkillPromotionConfig, pure functions, DB status methods
  - phase: 05-memory-consolidation
    provides: consolidation pipeline, ExecutionTraceRow, PatternStore, send_refinement_llm_call
provides:
  - Four new consolidation sub-tasks (flag candidates, draft skills, mental tests, check promotions)
  - flag_skill_draft_candidates with separate watermark from trace review
  - draft_flagged_skill_candidates with hybrid LLM generation into drafts/ directory
  - run_skill_mental_tests with 3-scenario LLM evaluation gating Draft->Testing
  - check_skill_promotions with threshold-based lifecycle promotion
  - check_lifecycle_promotion_after_settle for eager promotion at settle time
  - list_consolidation_state_by_prefix and list_skill_variants_by_status DB helpers
  - parse_mental_test_results with JSON, code-block, and fallback parsing
affects: [06-03-PLAN, skill-evolution, heartbeat, consolidation]

# Tech tracking
tech-stack:
  added: []
  patterns: [hybrid-llm-template-generation, mental-test-evaluation, watermark-isolation, settle-time-eager-promotion]

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/consolidation.rs
    - crates/amux-daemon/src/agent/skill_discovery.rs
    - crates/amux-daemon/src/agent/skill_evolution.rs
    - crates/amux-daemon/src/history.rs

key-decisions:
  - "Separate skill_draft_watermark from trace_review_watermark per Pitfall 7 to avoid coupling"
  - "One LLM call per tick for both drafting and mental tests (budget-safe per Pitfall 4)"
  - "Override register_skill_document default status to draft per Pitfall 6"
  - "Mental test requires 2/3 scenario passes for Draft->Testing promotion"
  - "parse_mental_test_results handles JSON, code-block-wrapped JSON, and fallback text matching"
  - "send_refinement_llm_call promoted to pub(super) for cross-module reuse"

patterns-established:
  - "Watermark isolation: separate watermarks for independent scanning passes over the same trace table"
  - "Budget-safe LLM sub-tasks: check remaining budget >= 10s before attempting LLM calls"
  - "Settle-time eager promotion: complement periodic consolidation checks with immediate post-settle checks"

requirements-completed: [SKIL-01, SKIL-02, SKIL-04, SKIL-05]

# Metrics
duration: 7min
completed: 2026-03-23
---

# Phase 06 Plan 02: Skill Discovery Pipeline Summary

**Four consolidation sub-tasks wiring skill candidate flagging, hybrid LLM drafting, mental test evaluation, and threshold-based lifecycle promotions with WORM provenance**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-23T17:24:17Z
- **Completed:** 2026-03-23T17:31:54Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments
- Four new consolidation sub-tasks (5-8) wired into idle-time pipeline: flag candidates, draft skills, mental tests, check promotions
- Skill candidate flagging uses separate watermark and complexity+novelty filters
- Hybrid LLM skill drafting creates SKILL.md files in ~/.tamux/skills/drafts/ with proper DB registration
- Mental test evaluation gates Draft->Testing promotion via 3-scenario LLM assessment (2/3 required to pass)
- Threshold-based lifecycle promotion (Testing->Active->Proven->Canonical) at both consolidation time and settle time
- All lifecycle transitions logged via WORM provenance events (D-07)
- 659 total daemon tests with zero regressions (5 new tests for parse_mental_test_results)

## Task Commits

Each task was committed atomically:

1. **Task 1: Consolidation sub-tasks for skill candidate flagging and hybrid LLM drafting** - `9e86463` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/consolidation.rs` - Added four skill sub-tasks (5-8), updated provenance log to include skill fields
- `crates/amux-daemon/src/agent/skill_discovery.rs` - Added flag_skill_draft_candidates, draft_flagged_skill_candidates, run_skill_mental_tests, check_skill_promotions async methods; parse_mental_test_results helper with tests
- `crates/amux-daemon/src/agent/skill_evolution.rs` - Added check_lifecycle_promotion_after_settle for eager promotion at settle time
- `crates/amux-daemon/src/history.rs` - Added list_consolidation_state_by_prefix and list_skill_variants_by_status DB helper methods

## Decisions Made
- Separate skill_draft_watermark from trace_review_watermark to avoid coupling between heuristic promotion and skill candidate scanning (Pitfall 7)
- One LLM call maximum per tick for drafting and mental tests to stay within 30-second consolidation budget (Pitfall 4)
- Override register_skill_document default status from "active" to "draft" immediately after registration (Pitfall 6)
- Mental test requires 2/3 passing scenarios for promotion -- balances quality gate with reasonable pass rate
- parse_mental_test_results handles three response formats: direct JSON, code-block-wrapped JSON, and fallback text matching for robustness
- send_refinement_llm_call promoted from private to pub(super) for cross-module reuse by skill_discovery.rs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Promoted send_refinement_llm_call visibility to pub(super)**
- **Found during:** Task 1 (skill_discovery.rs calling consolidation.rs method)
- **Issue:** send_refinement_llm_call was private to consolidation.rs, but skill_discovery.rs needs to call it
- **Fix:** Changed `async fn send_refinement_llm_call` to `pub(super) async fn send_refinement_llm_call`
- **Files modified:** crates/amux-daemon/src/agent/consolidation.rs
- **Verification:** cargo build -p tamux-daemon succeeds
- **Committed in:** 9e86463 (Task 1 commit)

**2. [Rule 1 - Bug] Added type annotations for closure parameters**
- **Found during:** Task 1 (compilation of skill_discovery.rs)
- **Issue:** Rust compiler could not infer types for closures in chained iterator calls
- **Fix:** Added explicit type annotations (`String`, `&str`, etc.) to closure parameters and return bindings
- **Files modified:** crates/amux-daemon/src/agent/skill_discovery.rs
- **Verification:** cargo build -p tamux-daemon succeeds
- **Committed in:** 9e86463 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both essential for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all methods are fully implemented with real logic (no placeholder returns or hardcoded empty values).

## Next Phase Readiness
- All consolidation pipeline integration complete for Plan 03 (skill discovery feed/surfacing)
- Lifecycle promotions work at both periodic consolidation time and eager settle time
- Drafts directory structure established for skill file management
- parse_mental_test_results robust enough for varied LLM response formats

## Self-Check: PASSED

All 5 files verified present. Commit 9e86463 verified in git log. 659 tests passing, 0 failures.

---
*Phase: 06-skill-discovery*
*Completed: 2026-03-23*
