---
name: frontend-ui-task
description: Use before implementing visible UI, interaction behavior, frontend flows, or Electron screens.
recommended_skills:
  - brainstorming
  - test-driven-development
  - playwright
  - verification-before-completion
---

# Frontend UI Task Guideline

Frontend work must be evaluated as a user workflow, not just as code that compiles.

## Workflow

1. Identify the primary user workflow and the screen states it passes through.
2. Follow existing design system, spacing, typography, icons, and interaction conventions.
3. Define states before implementation: loading, empty, populated, error, disabled, focus, hover, narrow viewport, and long content.
4. Use stable layout dimensions for controls and repeated UI so text and state changes do not shift or overlap.
5. Prefer real product/workflow UI over explanatory landing pages.
6. Test behavior with lint/build and, when visual behavior matters, a browser or Electron smoke check.

## Quality Gate

Do not finish visible UI work without checking that text fits, controls remain usable, and the affected workflow can be completed.
