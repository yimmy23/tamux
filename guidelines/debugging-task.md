---
name: debugging-task
description: Use when investigating failing behavior, regressions, crashes, flaky tests, or unexpected output.
recommended_skills:
  - systematic-debugging
recommended_guidelines:
  - general-programming
  - testing-task
  - ci-failure-task
---

## Overview

Debugging requires a systematic approach — guessing is inefficient and unreliable.

## Workflow

1. Reproduce the issue reliably before trying to fix it.
2. Isolate the smallest failing case.
3. Form a hypothesis before making changes — test one assumption at a time.
4. Add targeted logging or assertions, not random print statements.
5. Check the obvious causes first: inputs, configuration, environment, recent changes.
6. Use `systematic-debugging` for structured debugging procedures.
7. Once fixed, verify the fix and add a regression test.

## Quality Gate

A debug is complete when the root cause is identified, the fix is verified, and a regression test prevents recurrence.