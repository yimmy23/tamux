---
name: debugging-task
description: Use when investigating failing behavior, regressions, crashes, flaky tests, or unexpected output.
recommended_skills:
  - systematic-debugging
  - verification-before-completion
---

# Debugging Task Guideline

Debugging work should prove a cause before changing code.

## Workflow

1. Reproduce the failure or collect direct evidence from logs, tests, traces, or user-visible behavior.
2. State the observed symptom separately from assumptions.
3. Form the smallest plausible hypothesis and test it.
4. Inspect the code path that produces the symptom, including callers, state setup, async boundaries, persistence, and error handling.
5. Fix the root cause, not only the visible symptom.
6. Add or update a regression test where the defect can reasonably recur.
7. Verify the original reproduction now passes.

## Quality Gate

Do not claim a fix without evidence that the original failure mode was exercised after the change.
