---
name: ci-failure-task
description: Use when diagnosing or fixing failing CI, release, packaging, or automation checks.
recommended_skills:
  - github:gh-fix-ci
  - systematic-debugging
  - verification-before-completion
---

# CI Failure Task Guideline

CI work should identify the failing check and reproduce the relevant part locally when practical.

## Workflow

1. Locate the failing job, step, command, and error message.
2. Distinguish infrastructure failure from code failure.
3. Reproduce the smallest failing command locally when dependencies are available.
4. Inspect recent changes to scripts, workflows, lockfiles, environment assumptions, and platform-specific paths.
5. Fix the narrow cause and avoid weakening CI unless the check is demonstrably wrong.
6. Run the closest local equivalent and explain any CI-only verification that remains.

## Quality Gate

Do not mark CI fixed from a code edit alone; provide the command or reasoning that exercises the failing path.
