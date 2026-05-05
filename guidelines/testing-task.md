---
name: testing-task
description: Use when adding, fixing, selecting, or interpreting tests and verification commands.
recommended_skills:
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
recommended_guidelines:
  - general-programming
  - coding-task
  - ci-failure-task
---

## Overview

Tests should cover meaningful risk, not just increase count. This ensures tests protect against real regressions.

## Workflow

1. Identify the behavior, contract, or failure mode being tested.
2. Cover happy path, boundary, invalid input, ordering, state, persistence, and regression cases.
3. Prefer deterministic tests with clear failure messages.
4. Keep fixtures small but representative.
5. Run focused tests first, then broader suites when blast radius warrants.
6. If tests are flaky, debug the flake instead of rerunning until green.

## Quality Gate

Do not accept a single narrow happy-path test as sufficient proof for a broad behavior change.