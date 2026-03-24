---
phase: 09-distribution
plan: 04
subsystem: cli
tags: [setup-wizard, first-run, provider-config, crossterm, reqwest, atomic-write]

# Dependency graph
requires:
  - phase: 01-production-foundation
    provides: "amux-protocol config utilities (amux_data_dir, ensure_amux_data_dir)"
provides:
  - "First-run setup wizard detecting missing config and guiding provider configuration"
  - "12 LLM provider presets (anthropic, openai, openrouter, groq, together, fireworks, deepseek, mistral, featherless, ollama, lmstudio, custom)"
  - "Masked API key input via crossterm raw mode"
  - "Atomic config write (temp file + rename) per Pitfall 6"
  - "Connectivity test against selected provider endpoint"
  - "CLI first-run intercept with daemon auto-start and client launch"
  - "Explicit 'tamux setup' command for reconfiguration"
affects: [10-concierge-onboarding]

# Tech tracking
tech-stack:
  added: [crossterm 0.28, reqwest (workspace), tempfile 3 (dev)]
  patterns: [atomic-config-write, first-run-detection, masked-terminal-input]

key-files:
  created:
    - crates/amux-cli/src/setup_wizard.rs
  modified:
    - crates/amux-cli/src/main.rs
    - crates/amux-cli/Cargo.toml

key-decisions:
  - "Async setup wizard using reqwest (not blocking) since CLI already runs under tokio"
  - "Optional subcommand (Option<Commands>) to detect bare 'tamux' invocations"
  - "Local providers (ollama, lmstudio) skip API key prompt automatically"
  - "Used amux_data_dir() (the re-exported public function) rather than tamux_data_dir()"

patterns-established:
  - "Atomic config write: temp file + rename for crash-safe config persistence"
  - "First-run detection: check for missing/empty provider in agent config.json"

requirements-completed: [DIST-05]

# Metrics
duration: 4min
completed: 2026-03-24
---

# Phase 9 Plan 4: Setup Wizard Summary

**First-run setup wizard with 12 LLM providers, masked API key input, connectivity test, atomic config write, and CLI entry point intercept with daemon auto-start**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-24T06:37:37Z
- **Completed:** 2026-03-24T06:41:45Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Created setup_wizard.rs module with needs_setup() detection and full interactive wizard flow
- 12 LLM providers supported: anthropic, openai, openrouter, groq, together, fireworks, deepseek, mistral, featherless, ollama, lmstudio, custom
- Masked API key input via crossterm raw mode (local providers skip API key)
- Atomic config write using temp file + rename per Pitfall 6
- Connectivity test against selected provider endpoint (handles success, auth errors, connection failures gracefully)
- CLI entry point wired: bare `tamux` with no config triggers wizard, auto-starts daemon, launches preferred client
- Explicit `tamux setup` command for reconfiguration
- 9 unit tests covering needs_setup logic, provider list, atomic write

## Task Commits

Each task was committed atomically:

1. **Task 1: Create setup wizard module with first-run detection and provider configuration** - `dc581dd` (feat)
2. **Task 2: Wire setup wizard into CLI entry point with daemon auto-start** - `642c26e` (feat)

## Files Created/Modified
- `crates/amux-cli/src/setup_wizard.rs` - First-run setup wizard: needs_setup(), run_setup_wizard(), 12 providers, connectivity test, atomic config write
- `crates/amux-cli/src/main.rs` - Optional subcommand, first-run intercept, Setup command, daemon auto-start, client launch
- `crates/amux-cli/Cargo.toml` - Added crossterm, reqwest, tempfile dependencies

## Decisions Made
- Used async reqwest for connectivity test since CLI already uses tokio runtime (no blocking feature needed)
- Made subcommand optional via `Option<Commands>` to detect bare `tamux` invocations without breaking existing command dispatch
- Local providers (ollama, lmstudio) skip API key prompt automatically via `is_local_provider()` check
- Used `amux_data_dir()` (the public re-export) rather than `tamux_data_dir()` which is not exported from amux-protocol lib.rs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Used amux_data_dir() instead of tamux_data_dir()**
- **Found during:** Task 1 (setup_wizard.rs compilation)
- **Issue:** `tamux_data_dir()` is not re-exported from amux-protocol's lib.rs; only `amux_data_dir()` is public
- **Fix:** Replaced all references to `amux_protocol::tamux_data_dir()` with `amux_protocol::amux_data_dir()`
- **Files modified:** crates/amux-cli/src/setup_wizard.rs
- **Verification:** `cargo check -p tamux-cli` passes
- **Committed in:** dc581dd (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Trivial API name adjustment. No scope creep.

## Issues Encountered
None beyond the deviation noted above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Setup wizard complete, ready for Phase 10 concierge onboarding to build on top
- All 4 Phase 9 plans now have summaries

## Self-Check: PASSED

- All created files verified to exist on disk
- Both task commits (dc581dd, 642c26e) verified in git history
- All 9 unit tests pass

---
*Phase: 09-distribution*
*Completed: 2026-03-24*
