---
name: dependency-upgrade-task
description: Use for upgrading packages, libraries, models, runtimes, toolchains, or lockfiles.
recommended_skills:
  - systematic-debugging
  - test-driven-development
  - verification-before-completion
---

# Dependency Upgrade Task Guideline

Dependency upgrades should be scoped and reversible.

## Workflow

1. Identify why the upgrade is needed: security, compatibility, feature, deprecation, or maintenance.
2. Read release notes and migration guides for breaking changes.
3. Update the smallest reasonable dependency set.
4. Check lockfile, generated files, runtime constraints, and transitive impacts.
5. Run tests covering affected integrations.
6. Document behavior changes and rollback path.

## Quality Gate

Do not batch unrelated dependency upgrades unless the user explicitly wants a broad maintenance pass.
