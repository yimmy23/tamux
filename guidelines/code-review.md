---
name: code-review
description: Use when reviewing local changes, pull requests, diffs, or proposed patches.
recommended_skills:
  - review
recommended_guidelines:
  - general-programming
  - testing-task
  - refactoring-task
---

## Overview

Code review catches bugs, ensures consistency, and shares knowledge across the team.

## Workflow

1. Understand the purpose of the change before reviewing the code.
2. Review the diff first, then check surrounding context if needed.
3. Focus on correctness, security, and maintainability — style is secondary.
4. Leave specific, actionable comments — not vague criticism.
5. Distinguish between blockers and suggestions clearly.
6. Verify tests cover the claimed behavior changes.
7. Approve only when you understand the code well enough to defend it.

## Quality Gate

A review is complete when all blocker items are resolved and the reviewer understands and approves the change.