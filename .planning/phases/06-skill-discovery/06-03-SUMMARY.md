---
phase: 06-skill-discovery
plan: 03
subsystem: agent
tags: [skill-discovery, concierge, heartbeat-digest, ipc-protocol, cli, skill-management]

# Dependency graph
requires:
  - phase: 06-skill-discovery
    plan: 01
    provides: SkillMaturityStatus, HeartbeatCheckType::SkillLifecycle, update_skill_variant_status, get_skill_variant
  - phase: 06-skill-discovery
    plan: 02
    provides: draft_flagged_skill_candidates, check_skill_promotions, check_lifecycle_promotion_after_settle, list_skill_variants_by_status
provides:
  - announce_skill_draft() and announce_skill_promotion() concierge methods on AgentEngine
  - SkillList, SkillInspect, SkillReject, SkillPromote ClientMessage IPC variants
  - SkillListResult, SkillInspectResult, SkillActionResult DaemonMessage IPC variants
  - SkillVariantPublic protocol struct for cross-crate skill data transfer
  - CLI `tamux skill list|inspect|reject|promote` subcommands
  - Server-side IPC handlers for all four skill management operations
affects: [07-community-skills, frontend-skill-panel, tui-skill-view]

# Tech tracking
tech-stack:
  added: []
  patterns: [concierge-workflow-notice-for-milestones, heartbeat-digest-for-lifecycle-events, dual-notification-for-major-promotions]

key-files:
  created: []
  modified:
    - crates/amux-daemon/src/agent/concierge.rs
    - crates/amux-daemon/src/agent/skill_discovery.rs
    - crates/amux-daemon/src/agent/skill_evolution.rs
    - crates/amux-protocol/src/messages.rs
    - crates/amux-daemon/src/server.rs
    - crates/amux-cli/src/main.rs
    - crates/amux-cli/src/client.rs

key-decisions:
  - "Skill announcement methods placed on AgentEngine via impl block in concierge.rs (not ConciergeEngine)"
  - "Minor promotions use HeartbeatDigest only; canonical promotions use both HeartbeatDigest and WorkflowNotice"
  - "SkillVariantPublic derives PartialEq for test assertions in bincode round-trip tests"
  - "Removed serde skip_serializing_if from bincode-serialized protocol variants (incompatible with positional encoding)"
  - "Skill reject only allowed for draft/testing status; archived status used instead of hard delete"

patterns-established:
  - "Dual notification pattern: HeartbeatDigest for routine events, both HeartbeatDigest+WorkflowNotice for major milestones"
  - "IPC skill management pattern: SkillList/SkillInspect/SkillReject/SkillPromote following AuditQuery/AuditDismiss convention"

requirements-completed: [SKIL-03, SKIL-06]

# Metrics
duration: 9min
completed: 2026-03-23
---

# Phase 06 Plan 03: Skill Discovery Feed and CLI Management Summary

**Concierge skill announcements with HeartbeatDigest lifecycle events, IPC protocol extensions for skill management, and `tamux skill` CLI subcommand with list/inspect/reject/promote operations**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-23T17:34:08Z
- **Completed:** 2026-03-23T17:43:55Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Skill drafts trigger concierge chat messages ("I noticed a new pattern in your work -- drafted a skill: {name}")
- Skill promotions appear in HeartbeatDigest items with SkillLifecycle check type and inline explanation
- Canonical promotions get prominent dual notification (HeartbeatDigest + WorkflowNotice)
- Full IPC protocol support with 4 request types and 3 response types, verified by 10 bincode round-trip tests
- CLI `tamux skill list` shows tabular output with status, name, uses, success rate, and tags
- CLI `tamux skill inspect` displays full skill detail including SKILL.md content from disk
- CLI `tamux skill reject` removes draft/testing skills with status archived to DB
- CLI `tamux skill promote` fast-promotes skills to target status with WORM provenance logging
- 669 total tests (659 daemon + 10 protocol) with zero regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Concierge skill announcements and HeartbeatDigest skill events** - `7b1e8da` (feat)
2. **Task 2: IPC protocol messages and CLI skill subcommand** - `b5ee108` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/agent/concierge.rs` - Added announce_skill_draft() and announce_skill_promotion() methods on AgentEngine
- `crates/amux-daemon/src/agent/skill_discovery.rs` - Wired announce_skill_draft after drafting, announce_skill_promotion after check_skill_promotions
- `crates/amux-daemon/src/agent/skill_evolution.rs` - Wired announce_skill_promotion after check_lifecycle_promotion_after_settle
- `crates/amux-protocol/src/messages.rs` - SkillList/SkillInspect/SkillReject/SkillPromote messages, SkillVariantPublic struct, 10 round-trip tests
- `crates/amux-daemon/src/server.rs` - IPC handlers for all four skill management operations with variant lookup and provenance
- `crates/amux-cli/src/main.rs` - Skill subcommand with SkillAction enum (List, Inspect, Reject, Promote)
- `crates/amux-cli/src/client.rs` - send_skill_list, send_skill_inspect, send_skill_reject, send_skill_promote client functions

## Decisions Made
- Skill announcement methods on AgentEngine via impl block in concierge.rs, following the pattern of split impl blocks across modules
- Minor promotions (testing->active, active->proven) emit HeartbeatDigest only; canonical promotions (proven->canonical) emit both HeartbeatDigest AND WorkflowNotice for prominent treatment per D-08
- SkillVariantPublic derives PartialEq for test assertions (not needed for other protocol types)
- Removed `#[serde(default, skip_serializing_if = "Option::is_none")]` from bincode-serialized protocol message fields since bincode requires positional encoding
- Skill reject uses "archived" status rather than hard deleting the DB record, preserving audit trail

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed serde skip_serializing_if from bincode-encoded fields**
- **Found during:** Task 2 (bincode round-trip tests)
- **Issue:** `#[serde(default, skip_serializing_if = "Option::is_none")]` on ClientMessage::SkillList.status and DaemonMessage::SkillInspectResult fields caused bincode deserialization failures (InvalidTagEncoding, UnexpectedEof)
- **Fix:** Removed serde skip/default attributes from all bincode-serialized protocol enum variant fields
- **Files modified:** crates/amux-protocol/src/messages.rs
- **Verification:** All 10 bincode round-trip tests pass
- **Committed in:** b5ee108 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential for correct protocol serialization. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Known Stubs
None - all methods are fully implemented with real logic (no placeholder returns or hardcoded empty values).

## Next Phase Readiness
- Skill discovery pipeline complete end-to-end: flagging, drafting, mental tests, promotions, announcements, and CLI management
- Ready for Phase 07 (Community Skills) to add publish/subscribe/discover over the existing skill variant infrastructure
- Frontend skill panel can be built using the SkillVariantPublic protocol type and existing IPC messages

## Self-Check: PASSED

All 7 modified files verified present. Commits 7b1e8da and b5ee108 verified in git log. 669 tests passing (659 daemon + 10 protocol), 0 failures.

---
*Phase: 06-skill-discovery*
*Completed: 2026-03-23*
