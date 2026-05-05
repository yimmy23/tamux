---
name: dependency-upgrade-task
description: Use for upgrading packages, libraries, models, runtimes, toolchains, or lockfiles.
recommended_skills:
  - testing
  - systematic-debugging
recommended_guidelines:
  - general-programming
  - testing-task
  - coding-task
---
## Overview

Dependency upgrades carry risk. This guideline structures safe upgrade workflows that catch regressions before merging.

## Workflow

1. Read the changelog and release notes for all versions between current and target.
2. Identify breaking changes, deprecated APIs, and behavioral differences.
3. Check the dependency's own dependency tree for conflicts.
4. Run the existing test suite before upgrading.
5. Upgrade in a focused branch with a clear test plan.
6. Run the full test suite after upgrade. Compare before/after test results.
7. Check compilation/build warnings for deprecated API usage.

## Quality Gate

Do not merge a dependency upgrade that breaks existing tests. If the upgrade changes behavior, document what changed and why.