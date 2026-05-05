---
name: refactoring-task
description: Use when reorganizing code without intentionally changing behavior.
recommended_skills:
  - test-driven-development
  - systematic-debugging
recommended_guidelines:
  - general-programming
  - coding-task
  - testing-task
---

## Overview

Refactoring changes internal structure while preserving external behavior. Tests are essential.

## Workflow

1. Ensure comprehensive test coverage of the area being refactored before starting.
2. Identify the specific improvement: readability, performance, maintainability, or architecture.
3. Make one logical change at a time — do not combine refactoring with feature work.
4. Run tests after each change to confirm behavior is preserved.
5. Keep refactoring commits separate from other changes in version control.
6. If refactoring reveals missing tests, add them before proceeding.
7. After refactoring, verify the system still works end-to-end.

## Quality Gate

Refactoring is complete when behavior is unchanged and tests pass. If you changed behavior, that's not refactoring — it's a feature.