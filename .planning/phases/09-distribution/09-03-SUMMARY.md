---
phase: 09-distribution
plan: 03
subsystem: infra
tags: [install-script, shell, powershell, binary-distribution, sha256, path-patching]

# Dependency graph
requires:
  - phase: 09-distribution
    provides: "GitLab CI pipeline and release artifact naming conventions (09-02)"
provides:
  - "Unix install script (curl-pipe installer) for Linux and macOS"
  - "PowerShell install script for Windows with admin elevation"
  - "SHA256 checksum verification on all binary downloads"
  - "Automatic PATH configuration for both Unix and Windows"
affects: [09-distribution, documentation, first-run-experience]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "POSIX sh installer with platform detection via uname"
    - "PowerShell installer with RuntimeInformation.OSArchitecture detection"
    - "Dual checksum tool support (sha256sum + shasum fallback)"

key-files:
  created:
    - scripts/install.sh
    - scripts/install.ps1
  modified: []

key-decisions:
  - "POSIX sh (not bash) for install.sh to maximize portability across Unix systems"
  - "Separate SHA256SUMS file per platform target for targeted verification"
  - "Desktop app offered as optional prompt only when stdin is terminal (non-interactive pipe skips silently)"

patterns-established:
  - "Install scripts use --dry-run flag for safe testing without downloads"
  - "TAMUX_VERSION env var for pinning specific versions in both scripts"
  - "GitLab Generic Package Registry URL pattern: packages/generic/tamux/{version}/{artifact}"

requirements-completed: [DIST-03, DIST-04]

# Metrics
duration: 2min
completed: 2026-03-24
---

# Phase 9 Plan 3: Platform Install Scripts Summary

**Unix and Windows install scripts with SHA256 verification, PATH auto-configuration, and optional desktop app download for tamux binary distribution**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-24T06:37:30Z
- **Completed:** 2026-03-24T06:39:38Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Unix install script detects platform (Linux/macOS, x64/arm64), downloads binaries from GitLab, verifies SHA256 checksums, installs to ~/.local/bin, patches shell profiles for PATH
- PowerShell installer with admin elevation installs to C:\Program Files\tamux, verifies checksums via Get-FileHash, updates system PATH persistently and in current session
- Both scripts support --dry-run mode and version pinning via TAMUX_VERSION env var

## Task Commits

Each task was committed atomically:

1. **Task 1: Create Unix install script (install.sh)** - `f44dfb3` (feat)
2. **Task 2: Create PowerShell install script (install.ps1)** - `160edb1` (feat)

## Files Created/Modified
- `scripts/install.sh` - Unix installer: platform detection, binary download from GitLab, SHA256 verification, PATH patching, optional desktop app
- `scripts/install.ps1` - Windows installer: arch detection, admin-elevated install to Program Files, SHA256 verification, system PATH update

## Decisions Made
- Used POSIX sh (not bash) for install.sh to ensure maximum portability -- works on minimal Alpine/BusyBox environments, not just GNU bash
- Desktop app prompt in install.sh uses terminal detection (`[ -t 0 ]`) to skip silently when piped from curl, avoiding hung prompts
- PowerShell script uses `[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture` for reliable architecture detection instead of WMI queries
- SHA256SUMS file naming follows per-platform pattern (`SHA256SUMS-{target}.txt`) consistent with 09-02 CI pipeline artifact naming

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Install scripts ready for use once GitLab CI pipeline (09-02) publishes release artifacts
- Both scripts use placeholder PROJECT_ID that needs replacement with actual GitLab project ID when releases go live
- First-run setup wizard (09-04) will complement these scripts by guiding configuration after install

## Self-Check: PASSED

- FOUND: scripts/install.sh
- FOUND: scripts/install.ps1
- FOUND: .planning/phases/09-distribution/09-03-SUMMARY.md
- FOUND: commit f44dfb3 (Task 1)
- FOUND: commit 160edb1 (Task 2)

---
*Phase: 09-distribution*
*Completed: 2026-03-24*
