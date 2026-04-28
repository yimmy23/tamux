---
name: rust-programming
description: Use when implementing, reviewing, debugging, or refactoring Rust code.
recommended_skills:
  - rust-programming
  - rust-compiler-diagnostics
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
recommended_guidelines:
  - general-programming
---

# Rust Programming Guideline

Rust work should preserve correctness first, then make ownership, error behavior, and verification explicit.

## Workflow

1. Read the crate boundary, surrounding module, and existing error/test patterns before changing code.
2. Apply `general-programming` for SOLID checks, file size, dependency direction, and complete implementation expectations.
3. Define the behavior contract: ownership, lifetimes, state transitions, async cancellation, errors, persistence, and public API impact.
4. Use `rust-programming` for design and review heuristics before implementation.
5. Use `rust-compiler-diagnostics` for compiler, borrow checker, lifetime, and Clippy failures.
6. Write or update tests before behavior changes. Prefer focused unit tests for local logic and integration tests for CLI, IPC, persistence, routing, or policy contracts.
7. Keep APIs idiomatic: accept borrowed forms where possible, return `Result` for recoverable errors, keep trait bounds local, and avoid exposing implementation detail through public types.
8. Keep concurrency explicit: avoid holding locks across awaits, bound channels when producers can outrun consumers, and give spawned tasks clear lifecycle ownership.
9. Run focused verification first, then broaden to workspace-level commands when the blast radius warrants it.

## Rust Best Practices

- Keep files focused and maintainable by splitting large modules; aim for under 500 LOC per file.
- Prefer borrowed parameter types: `&str`, `&[T]`, `&Path`, or `impl AsRef<Path>` instead of `&String`, `&Vec<T>`, or `PathBuf` unless ownership is required.
- Use domain types for validated values instead of passing raw strings, IDs, and booleans through multiple layers.
- Preserve source errors with `#[source]`, `#[from]`, or explicit context; do not collapse structured failures into strings too early.
- Clone deliberately at task, channel, storage, and ownership boundaries. Shorten borrows before cloning inside a local synchronous flow.
- Avoid holding locks or mutable borrows across `.await`; extract the required value, release the guard, then await.
- Keep `#[allow(clippy::...)]` narrow and justified. A broad allow is usually a missed design decision.
- Prefer tests that assert observable behavior, error variants, and state transitions over tests that mirror implementation details.

## Quality Gate

Do not call Rust work complete unless the code is formatted, compiler-clean, and verified with relevant tests. If `cargo clippy --workspace --all-targets -- -D warnings` is skipped, state why.

## Source Basis

This guideline is informed by the MIT-licensed `udapy/rust-agentic-skills` project and official Rust resources: the Rust API Guidelines, The Rust Programming Language, rustc diagnostics, and Clippy documentation.
