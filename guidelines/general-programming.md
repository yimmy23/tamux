---
name: general-programming
description: Use as the baseline guideline for implementing, reviewing, or refactoring production code in any programming language.
recommended_skills:
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
---

# General Programming Guideline

Programming work should produce small, understandable units with clear contracts, meaningful tests, and no hidden follow-up work.

## Core Principles

1. Read the surrounding code and project instructions before choosing an approach.
2. Keep each file under 500 lines of code. Split by responsibility before a file reaches that size.
3. Prefer clear names, explicit data flow, and simple control flow over cleverness.
4. Keep changes scoped to the requested behavior and nearby enabling cleanup.
5. Implement completely. Do not leave placeholders, stubs, TODO-driven behavior, or "finish later" paths.
6. Make errors observable and actionable at the layer that can handle them.
7. Protect public contracts. When a contract must change, update callers, docs, and tests together.

## SOLID Checks

- **Single Responsibility**: A module, type, or function should have one reason to change. Split orchestration, validation, persistence, rendering, transport, and domain logic when they start competing.
- **Open/Closed**: Add behavior through well-defined extension points where the domain is likely to vary. Do not add abstraction just because variation is imaginable.
- **Liskov Substitution**: Subtypes, trait implementations, and adapters must preserve caller expectations. Do not weaken invariants, hide failures, or change semantic meaning behind the same interface.
- **Interface Segregation**: Keep interfaces small and role-focused. Callers should not depend on methods, fields, or capabilities they do not use.
- **Dependency Inversion**: High-level policy should depend on narrow abstractions, not concrete infrastructure. Keep database, network, filesystem, clock, and UI details at boundaries.

## Design Workflow

1. Define the behavior contract: inputs, outputs, state, errors, permissions, persistence, concurrency, and external side effects.
2. Identify likely failure modes before coding: invalid input, missing data, ordering, retries, cancellation, partial failure, race conditions, and large inputs.
3. Choose the smallest coherent design that satisfies the contract.
4. Keep pure domain logic separate from I/O and framework code when practical.
5. Prefer composition over inheritance or large shared base types.
6. Avoid global mutable state unless the lifecycle and synchronization are explicit.
7. Keep dependencies directed inward: UI and adapters call application/domain code, not the other way around.

## Implementation Practices

- Extract a helper when it names a real concept, removes meaningful duplication, or isolates a contract.
- Avoid boolean parameter clusters; use named options or domain types when combinations matter.
- Validate at boundaries and represent validated data with stronger types or well-named structures.
- Keep concurrency ownership explicit. Shared mutable state needs a clear owner, lock scope, and cancellation story.
- Treat logs and metrics as part of operability, but keep secrets and personal data out of them.
- Use generated code only when the source of truth and regeneration command are clear.

## Testing And Verification

1. Test behavior, not implementation shape.
2. Cover happy path, boundary cases, invalid input, error handling, ordering, and regression risk.
3. Keep fixtures small but representative.
4. Run focused verification first, then broader tests when the blast radius warrants it.
5. Do not claim completion until formatting, lint/build, and relevant tests have fresh evidence.

## Quality Gate

Do not call programming work complete if any new or modified file exceeds 500 LOC, if behavior is only partially implemented, or if the verification does not cover the main risks.
