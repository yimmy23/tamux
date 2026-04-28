---
name: refactoring-task
description: Use when reorganizing code without intentionally changing behavior.
recommended_skills:
  - component-refactoring
  - test-driven-development
  - verification-before-completion
recommended_guidelines:
  - general-programming
---

# Refactoring Task Guideline

Refactoring should preserve behavior while making the next change easier.

## Workflow

1. Identify the behavior that must remain unchanged.
2. Apply `general-programming` for SOLID checks, file boundaries, and dependency direction.
3. Run or add characterization tests before moving logic when risk is meaningful.
4. Keep the refactor scoped to the stated goal and nearby enabling cleanup.
5. Move code in small steps with verification between risky boundaries.
6. Preserve public contracts unless the user explicitly requested a contract change.
7. Avoid mixing refactoring with feature work unless the refactor is necessary to implement the feature safely.

## Quality Gate

Do not call a refactor behavior-preserving unless relevant tests or manual checks support that claim.
