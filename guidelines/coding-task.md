---
name: coding-task
description: Use before implementing a feature, behavior change, or bug fix in code.
recommended_skills:
  - brainstorming
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
---

# Coding Task Guideline

Use this guideline to orchestrate coding work. Skills remain the source of detailed procedure; this document decides which work shape is expected.

## Workflow

1. Read the project instructions and inspect the current implementation before choosing an approach.
2. Identify the behavioral contract: inputs, outputs, state transitions, persistence, permissions, errors, and external integration points.
3. Build a risk and test matrix before implementation. Cover happy path, boundary cases, invalid input, regression risk, concurrency or ordering, persistence, UI state, and integration contracts where relevant.
4. Use `test-driven-development` unless the user explicitly asks for another technique or the change is purely generated/configuration-only.
5. Write tests that represent meaningful behavior, not just one narrow success case.
6. Implement the smallest coherent change that satisfies the matrix.
7. Run focused tests first, then the broader relevant verification command.
8. Before claiming completion, use `verification-before-completion` and report what was actually run.

## Quality Gate

Do not call the task complete until the likely failure spectrum has been considered and the selected verification covers the meaningful risks. If some risks are intentionally not tested, say why.
