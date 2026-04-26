---
name: refactoring-task
description: Use when reorganizing code without intentionally changing behavior.
recommended_skills:
  - component-refactoring
  - test-driven-development
  - verification-before-completion
---

# Refactoring Task Guideline

Refactoring should preserve behavior while making the next change easier.

## Workflow

1. Identify the behavior that must remain unchanged.
2. Run or add characterization tests before moving logic when risk is meaningful.
3. Keep the refactor scoped to the stated goal and nearby enabling cleanup.
4. Move code in small steps with verification between risky boundaries.
5. Preserve public contracts unless the user explicitly requested a contract change.
6. Avoid mixing refactoring with feature work unless the refactor is necessary to implement the feature safely.

## Quality Gate

Do not call a refactor behavior-preserving unless relevant tests or manual checks support that claim.
