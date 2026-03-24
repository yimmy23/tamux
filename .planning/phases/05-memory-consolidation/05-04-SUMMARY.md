---
phase: 05-memory-consolidation
plan: 04
subsystem: agent
tags: [consolidation, memory, heuristics, system-prompt, tool-weighting, llm-refinement, persistence]

# Dependency graph
requires:
  - phase: 05-memory-consolidation
    plan: 02
    provides: Consolidation tick framework with 3 sub-tasks, supersede_memory_fact, extract_memory_fact_candidates
  - phase: 05-memory-consolidation
    plan: 03
    provides: persist_learning_stores(), HeuristicStore/PatternStore persistence infrastructure
provides:
  - Learned Patterns system prompt section from HeuristicStore (D-08 mechanism 1)
  - Tool selection weighting by effectiveness_score in tool_executor.rs (D-08 mechanism 2)
  - LLM-powered memory refinement sub-task in consolidation (D-12, MEMO-08)
  - Learning store persistence after consolidation trace review
  - build_learned_patterns_section helper for prompt injection
  - reorder_tools_by_heuristics for tool ordering by task type
affects: [agent-loop, system-prompt, tool-selection]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Dual heuristic influence: system prompt injection + tool selection weighting (D-08)"
    - "LLM-powered memory refinement with budget-aware timeout and circuit breaker gating"
    - "Heuristic reliability threshold: usage_count >= 5 and effectiveness >= 0.6 for prompt injection"
    - "Provider-agnostic refinement LLM call following memory_flush.rs pattern"

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/system_prompt.rs
    - crates/amux-daemon/src/agent/tool_executor.rs
    - crates/amux-daemon/src/agent/agent_loop.rs
    - crates/amux-daemon/src/agent/consolidation.rs

key-decisions:
  - "Learned patterns threshold: usage_count >= 5 AND effectiveness >= 0.6 (only reliable heuristics shown)"
  - "Tool reorder uses stable sort with -1.0 sentinel for tools without heuristic data (preserves original order)"
  - "Memory refinement handles one conflict group per consolidation tick (budget-safe)"
  - "send_refinement_llm_call uses provider's configured api_transport rather than hardcoded ChatCompletions"
  - "Circuit breaker checked via check_circuit_breaker method (consistent with memory_flush.rs pattern)"

patterns-established:
  - "System prompt extensible via optional parameters at the end of build_system_prompt signature"
  - "Tool reordering as a separate pure function in tool_executor.rs, wired in agent_loop.rs after filtering"
  - "Budget-gated LLM calls with minimum-seconds check before attempting"

requirements-completed: [MEMO-08]

# Metrics
duration: 9min
completed: 2026-03-23
---

# Phase 5 Plan 4: Heuristic Influence and Memory Refinement Summary

**D-08 dual heuristic influence (system prompt + tool weighting) and LLM-powered memory fact refinement with learning store persistence**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-23T14:42:51Z
- **Completed:** 2026-03-23T14:51:51Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- D-08 mechanism 1: Learned Patterns section injected into system prompt from HeuristicStore with reliability filters (usage >= 5, effectiveness >= 0.6), grouped by task type
- D-08 mechanism 2: reorder_tools_by_heuristics reorders tool list by effectiveness_score for the current task type, influencing LLM tool selection bias
- D-12/MEMO-08: Memory refinement detects contradictory/redundant facts via key overlap and merges via LLM call within consolidation budget
- Learning stores persisted after consolidation trace review for cross-session continuity
- Full consolidation pipeline complete: trace review -> decay -> tombstone cleanup -> refinement -> persistence

## Task Commits

Each task was committed atomically:

1. **Task 1: Learned Patterns system prompt injection and tool selection weighting** - `f7a953b` (feat)
2. **Task 2: LLM-powered memory refinement and learning store persistence** - `5fef093` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/system_prompt.rs` - Added learned_patterns parameter to build_system_prompt, Learned Patterns section injection, build_learned_patterns_section helper
- `crates/amux-daemon/src/agent/tool_executor.rs` - Added reorder_tools_by_heuristics function for D-08 tool weighting
- `crates/amux-daemon/src/agent/agent_loop.rs` - Wired learned patterns into build_system_prompt calls, wired tool reordering after tool filter
- `crates/amux-daemon/src/agent/consolidation.rs` - refine_memory_facts sub-task, send_refinement_llm_call LLM helper, persist_learning_stores call, updated provenance log

## Decisions Made
- Learned patterns only shown for heuristics with usage_count >= 5 AND effectiveness_score >= 0.6 to prevent noise from low-confidence data
- Tool reorder uses stable sort with -1.0 sentinel for tools without heuristic data, preserving original ordering for unknown tools
- Memory refinement handles one conflict group per consolidation tick to stay within the 30-second budget
- send_refinement_llm_call uses the provider's configured api_transport rather than hardcoding a specific transport
- Circuit breaker checked via self.check_circuit_breaker() method for consistency with memory_flush.rs pattern (not direct registry access)
- MemoryFactCandidate.display field used for fact text (not .value as plan pseudocode suggested -- adapted to actual struct shape)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed ToolDefinition field access in reorder_tools_by_heuristics**
- **Found during:** Task 1 (tool reordering implementation)
- **Issue:** Plan pseudocode used `tool.name` but ToolDefinition has the name at `tool.function.name`
- **Fix:** Changed to `a.function.name` and `b.function.name` in the sort comparator
- **Files modified:** crates/amux-daemon/src/agent/tool_executor.rs
- **Verification:** cargo build -p tamux-daemon compiles cleanly
- **Committed in:** f7a953b (Task 1 commit)

**2. [Rule 1 - Bug] Fixed circuit breaker access in refine_memory_facts**
- **Found during:** Task 2 (memory refinement implementation)
- **Issue:** Plan used `self.circuit_breakers.is_allowed()` but CircuitBreakerRegistry has no `is_allowed` method
- **Fix:** Used `self.check_circuit_breaker(&config.provider).await` which is the AgentEngine method used by memory_flush.rs
- **Files modified:** crates/amux-daemon/src/agent/consolidation.rs
- **Verification:** cargo build -p tamux-daemon compiles cleanly
- **Committed in:** 5fef093 (Task 2 commit)

**3. [Rule 1 - Bug] Adapted MemoryFactCandidate field names**
- **Found during:** Task 2 (memory refinement implementation)
- **Issue:** Plan pseudocode used `candidate.value` but actual struct has `candidate.display` and `candidate.normalized`
- **Fix:** Used `candidate.display` for fact text in both the LLM prompt and supersede call
- **Files modified:** crates/amux-daemon/src/agent/consolidation.rs
- **Verification:** cargo build -p tamux-daemon compiles cleanly
- **Committed in:** 5fef093 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 bugs -- all API mismatch between plan pseudocode and actual codebase)
**Impact on plan:** All fixes required for compilation correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 05 (memory-consolidation) fully complete: all 4 plans delivered
- Full learning loop closed: execution traces -> pattern mining -> heuristic promotion -> system prompt injection + tool weighting -> memory refinement -> persistence
- Ready for Phase 06 (Skill Discovery) which builds on the heuristic and pattern learning infrastructure

## Self-Check: PASSED

- All 4 modified files exist on disk
- Both task commits (f7a953b, 5fef093) verified in git log
- 626 daemon tests pass (12 consolidation, 13 heuristic, all others)
- Build compiles cleanly (no new warnings)

---
*Phase: 05-memory-consolidation*
*Completed: 2026-03-23*
