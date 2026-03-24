---
phase: 09-distribution
plan: 01
subsystem: distribution
tags: [npm, npx, postinstall, binary-download, sha256, cross-platform]

# Dependency graph
requires:
  - phase: 01-production-foundation
    provides: "Rust binaries (tamux-daemon, tamux, tamux-tui) built via cargo"
provides:
  - "npm-package/ directory with thin wrapper for npx tamux distribution"
  - "Postinstall binary downloader with SHA256 checksum verification"
  - "Bin wrapper with two-layer fallback (postinstall + runtime download)"
  - "Version sync in bump-version.sh for npm-package/package.json"
affects: [09-02-ci-pipeline, 09-04-first-run-wizard]

# Tech tracking
tech-stack:
  added: [npm-package]
  patterns: [postinstall-binary-download, two-layer-fallback, non-blocking-failure]

key-files:
  created:
    - npm-package/package.json
    - npm-package/install.js
    - npm-package/bin/tamux.js
    - npm-package/.npmignore
  modified:
    - scripts/bump-version.sh

key-decisions:
  - "Non-blocking failure: all postinstall errors exit 0 to never break npm install"
  - "Two-layer fallback: postinstall downloads first, bin wrapper retries at runtime if missing"
  - "install.js exported as module.exports for require() from bin wrapper fallback path"
  - "Poll-based binary wait in bin wrapper (500ms intervals, 60s max) for async download completion"

patterns-established:
  - "Non-blocking postinstall: catch all errors, warn, exit 0"
  - "Platform detection via os.platform()-os.arch() key mapped through PLATFORM_MAP"
  - "GitLab Generic Package Registry URL pattern: packages/generic/tamux/{version}/{artifact}"

requirements-completed: [DIST-01]

# Metrics
duration: 3min
completed: 2026-03-24
---

# Phase 9 Plan 1: npm Wrapper Package Summary

**Thin npm wrapper package enabling `npx tamux` with platform-specific binary download from GitLab Releases, SHA256 checksum verification, and two-layer fallback**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-24T06:37:31Z
- **Completed:** 2026-03-24T06:39:59Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- npm package manifest with bin entry, postinstall hook, platform/cpu constraints, and Node >=18 engine requirement
- Postinstall binary downloader handling 5 platform targets with SHA256 checksum verification and redirect-following HTTPS download
- Bin wrapper that spawns native binary with full arg/signal forwarding, falling back to runtime download if binary missing
- bump-version.sh updated to keep npm-package/package.json version in sync with workspace

## Task Commits

Each task was committed atomically:

1. **Task 1: Create npm package with postinstall binary downloader** - `f44dfb3` (feat)
2. **Task 2: Create bin wrapper with fallback download and update bump-version.sh** - `26470e7` (feat)

## Files Created/Modified
- `npm-package/package.json` - npm package manifest with tamux bin entry, postinstall hook, platform/cpu constraints
- `npm-package/install.js` - Postinstall script: platform detection, GitLab Releases download, SHA256 verification, tarball extraction
- `npm-package/bin/tamux.js` - Bin wrapper that locates native binary, spawns it with arg/signal forwarding, falls back to download
- `npm-package/.npmignore` - Excludes node_modules, tarballs, and test directory from npm publish
- `scripts/bump-version.sh` - Added npm-package/package.json to version sync targets

## Decisions Made
- Non-blocking failure model: all postinstall and checksum errors warn and exit 0 to never break `npm install` in CI or user environments
- Two-layer fallback per plan pitfall guidance: postinstall runs at install time, bin wrapper re-attempts download at runtime if binary is missing
- install.js uses `module.exports = main` to allow `require("../install")` from the bin wrapper for the fallback path
- Bin wrapper uses poll-based check (500ms intervals, 60s max) after triggering async download, since install.js runs asynchronously
- PROJECT_ID placeholder in download URL to be replaced during CI publish (avoids hardcoding numeric GitLab project ID)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Known Stubs
- `npm-package/install.js` line 21: `PROJECT_ID = "PROJECT_ID"` - Placeholder for GitLab project numeric ID; will be set during CI publish pipeline (Plan 09-02)

## Next Phase Readiness
- npm-package/ directory ready for CI pipeline integration (Plan 09-02) to publish to npm registry
- PROJECT_ID placeholder needs to be set during CI publish (Plan 09-02 scope)
- Plan 09-04 (first-run wizard) will hook into the tamux CLI invocation path that this bin wrapper enables

## Self-Check: PASSED

All 5 created/modified files verified on disk. Both task commits (f44dfb3, 26470e7) verified in git log.

---
*Phase: 09-distribution*
*Completed: 2026-03-24*
