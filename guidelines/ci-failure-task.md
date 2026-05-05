---
name: ci-failure-task
description: Use when diagnosing or fixing failing CI, release, packaging, or automation checks.
recommended_skills:
  - testing
  - systematic-debugging
recommended_guidelines:
  - testing-task
  - debugging-task
  - general-programming
---

## Overview

CI failures block the team. Fix them methodically, not by randomly rerunning.

## Workflow

1. Read the full CI log before taking any action — the error is usually near the end.
2. Distinguish between flaky tests, infrastructure failures, and real regressions.
3. For build failures: check dependencies, environment, and configuration changes.
4. For test failures: reproduce locally before attempting a fix.
5. For infrastructure failures: restart the CI job after verifying the infrastructure.
6. If a test is genuinely flaky, fix it or skip it — do not ignore it.
7. After fixing, ensure the fix is in the branch, not just in the CI environment.

## Quality Gate

A CI fix is complete when the pipeline passes and the root cause is documented.