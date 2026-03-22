# Testing Patterns

**Analysis Date:** 2026-03-22

---

## Overview

The project has two distinct testing contexts with different frameworks:

1. **Rust backend** (`crates/`) — inline `#[cfg(test)]` modules using the built-in Rust test harness, with `tokio` for async tests
2. **Frontend** (`frontend/`) — no test framework configured; no test files found

---

## Rust Test Framework

**Runner:** Rust built-in test harness (no external runner)

**Async tests:** `tokio` via `#[tokio::test]`

**Assertions:** Standard Rust macros (`assert_eq!`, `assert!`, `assert_ne!`) — no external assertion library

**Run Commands:**
```bash
cargo test                        # Run all tests in workspace
cargo test -p amux-daemon         # Run tests in specific crate
cargo test circuit_breaker        # Run tests matching a name pattern
cargo test -- --nocapture         # Show println! output during tests
```

**Total test count:** ~730 test functions across the workspace (`#[test]` count), plus 6 `#[tokio::test]` async tests.

---

## Test File Organization

**Pattern: Tests are co-located in the same file as the code being tested**, at the bottom of each `.rs` file.

**Naming:**
- Test modules are always `mod tests`
- Test functions use `snake_case` named for what they assert: `default_starts_closed`, `five_failures_trips_to_open`, `append_content_separates_blocks`

**Structure:**
```
crates/
  amux-daemon/src/agent/
    circuit_breaker.rs      # impl + #[cfg(test)] mod tests at bottom
    rate_limiter.rs         # impl + #[cfg(test)] mod tests at bottom
    memory.rs               # impl + #[cfg(test)] mod tests at bottom
    collaboration.rs        # impl + #[cfg(test)] mod tests at bottom
    compaction.rs           # impl + #[cfg(test)] mod tests at bottom
    subagent/
      tool_filter.rs        # impl + tests at bottom
      lifecycle.rs          # impl + tests at bottom
      context_budget.rs     # impl + tests at bottom
      tool_graph.rs         # impl + tests at bottom
      termination.rs        # impl + tests at bottom
      supervisor.rs         # impl + tests at bottom
    context/
      compression.rs        # impl + tests at bottom
      context_item.rs       # impl + tests at bottom
      archive.rs            # impl + tests at bottom
      audit.rs              # impl + tests at bottom
      restoration.rs        # impl + tests at bottom
    learning/
      heuristics.rs         # impl + tests at bottom
      traces.rs             # impl + tests at bottom
      patterns.rs           # impl + tests at bottom
      effectiveness.rs      # impl + tests at bottom
    metacognitive/
      self_assessment.rs    # impl + tests at bottom
      escalation.rs         # impl + tests at bottom
      replanning.rs         # impl + tests at bottom
      resource_alloc.rs     # impl + tests at bottom
    liveness/
      checkpoint.rs         # impl + tests at bottom
    concierge.rs            # impl + tokio::test block at bottom
    config.rs               # impl + tokio::test block at bottom
    mod.rs                  # re-exports + tests for task queue logic
  amux-gateway/src/
    router.rs               # impl + tests at bottom
```

---

## Test Structure

**Module declaration:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test functions follow
}
```

The `use super::*;` wildcard is used universally within test modules to access the module under test without explicit re-imports.

**Typical synchronous test:**
```rust
#[test]
fn five_failures_trips_to_open() {
    let mut cb = CircuitBreaker::default();
    for t in 1..=5 {
        cb.record_failure(t);
    }
    assert_eq!(cb.state(), CircuitState::Open);
}
```

**Typical async test:**
```rust
#[tokio::test]
async fn merge_config_patch_preserves_existing_provider_state() {
    let manager = SessionManager::new();
    let engine = AgentEngine::new(manager, AgentConfig::default());
    // ...
    engine.merge_config_patch_json(r#"{"model":"gpt-5.4-mini"}"#).await.unwrap();
    let updated = engine.get_config().await;
    assert_eq!(updated.model, "gpt-5.4-mini");
}
```

**Section dividers within large test modules:**
```rust
// ── TokenBucket tests ──────────────────────────────────────────────

#[test]
fn new_bucket_starts_full() { ... }

// ── RateLimiter tests ──────────────────────────────────────────────

#[test]
fn default_limiter_allows_normal_usage() { ... }
```
Example: `crates/amux-daemon/src/agent/rate_limiter.rs`

**Numbered sections in large test blocks:**
```rust
// -----------------------------------------------------------------------
// 1. Summarize produces tool call summary
// -----------------------------------------------------------------------
#[test]
fn summarize_produces_tool_call_summary() { ... }
```
Example: `crates/amux-daemon/src/agent/context/compression.rs`

**Inline section comments:**
```rust
// --- Construction ---
#[test]
fn allow_all_permits_everything() { ... }

// --- Whitelist-only ---
#[test]
fn whitelist_allows_listed_tools() { ... }
```
Example: `crates/amux-daemon/src/agent/subagent/tool_filter.rs`

---

## Helper Functions (Test Fixtures)

Tests use local helper functions (not external fixture libraries). They are defined inside the `mod tests` block:

**Pattern: `make_<type>` for lightweight struct construction:**
```rust
fn make_msg(text: &str) -> GatewayMessage {
    GatewayMessage {
        platform: "test".into(),
        channel_id: "ch-1".into(),
        user_id: "u-1".into(),
        text: text.into(),
        timestamp: 0,
    }
}
```
Examples: `crates/amux-gateway/src/router.rs`, `crates/amux-daemon/src/agent/subagent/tool_filter.rs`, `crates/amux-daemon/src/agent/context/context_item.rs`

**Pattern: `sample_<type>` for more complex domain objects:**
```rust
fn sample_provider_config() -> ProviderConfig {
    ProviderConfig {
        base_url: "https://example.invalid".to_string(),
        model: "test-model".to_string(),
        api_key: String::new(),
        ...
    }
}

fn sample_message(content: &str) -> AgentMessage {
    AgentMessage::user(content, 1)
}
```
Example: `crates/amux-daemon/src/agent/compaction.rs`

**Pattern: `default_config()` or `AgentConfig::default()` as a starting point:**
```rust
fn default_config() -> SupervisorConfig {
    SupervisorConfig { ... }
}
```
Then tests mutate specific fields to set up the scenario:
```rust
let mut config = AgentConfig::default();
config.max_context_messages = 3;
config.keep_recent_on_compact = 2;
```

**Pattern: `Convenience builder` with doc comment when inputs are complex:**
```rust
/// Convenience builder for test inputs.
fn make_input(
    goal_distance_pct: f64,
    steps_completed: usize,
    ...
) -> AssessmentInput {
    AssessmentInput { ... }
}
```
Example: `crates/amux-daemon/src/agent/metacognitive/self_assessment.rs`

---

## Mocking

**There is no mocking library** (no `mockall`, `mockito`, or similar) in the workspace.

Tests avoid mocking by:
1. Testing pure functions directly (most common — `circuit_breaker`, `rate_limiter`, `memory`, `tool_filter`)
2. Using real lightweight implementations (`SessionManager::new()` in async tests)
3. Passing fake/minimal data structures rather than mock objects
4. Using `broadcast::channel` and `CancellationToken` directly for concurrency tests

**What is NOT mocked:**
- File I/O (tests avoid file operations or use `/tmp`)
- Network calls (no integration tests hitting real APIs)
- `SessionManager` — the real struct is used in async tests for `AgentEngine`

---

## Assertion Patterns

**Equality assertions with descriptive failure messages:**
```rust
assert!(
    limiter.check("bash_command", now),
    "call {} should succeed",
    i
);
```

**Error message assertions:**
```rust
let err = validate_memory_size(MemoryTarget::Soul, &"x".repeat(1_501)).unwrap_err();
assert!(err.to_string().contains("SOUL.md would exceed its limit"));
```

**Floating-point equality uses epsilon comparison:**
```rust
assert!((h.success_rate - 2.0 / 3.0).abs() < 0.01);
assert!((a.min_progress_rate - 0.1).abs() < f64::EPSILON);
```

**Pattern matching as assertion (for enum variants):**
```rust
let GatewayAction::ManagedCommand(req) = route_message(&make_msg("!ls -la")).unwrap()
else {
    panic!("expected managed command");
};
assert_eq!(req.command, "ls -la");
```

**Chained operations before assertion:**
```rust
let second = apply_vote_to_disagreement(&mut disagreement, &agents, "b", "recommend", None)
    .expect("second vote should resolve");
assert_eq!(second.winner, "recommend");
```

---

## Async and Concurrency Testing

**`tokio::test` is only used when the function under test is async** (6 async tests total). Pure logic tests always use `#[test]`, even in async-heavy codebases.

**Cancellation testing:**
```rust
#[tokio::test]
async fn managed_command_wait_can_be_cancelled() {
    let (_tx, mut rx) = broadcast::channel(4);
    let token = CancellationToken::new();
    token.cancel();

    let error = wait_for_managed_command_outcome(&mut rx, SessionId::nil(), "exec-1", 30, Some(&token))
        .await
        .err()
        .expect("managed wait should abort when cancellation is requested");

    assert!(error.to_string().contains("cancelled"));
}
```

**Timeout testing** — time is injected as `u64` milliseconds rather than `Instant`, enabling deterministic tests without sleeping:
```rust
fn open_transitions_to_half_open_after_timeout() {
    let mut cb = CircuitBreaker::default();
    for t in 1..=5 { cb.record_failure(t); }
    // No sleep needed — time is passed as a parameter
    assert!(cb.can_execute(30_005));
    assert_eq!(cb.state(), CircuitState::HalfOpen);
}
```

---

## Coverage

**Requirements:** No coverage threshold configured; no CI coverage tooling detected.

**View Coverage:**
```bash
cargo tarpaulin --workspace        # if tarpaulin is installed
cargo llvm-cov --workspace         # if cargo-llvm-cov is installed
```

Neither `tarpaulin` nor `llvm-cov` appear in workspace deps — no enforced coverage.

---

## Test Types

**Unit Tests:**
- All tests are unit tests, testing individual functions and structs in isolation
- Located in `#[cfg(test)] mod tests` at the bottom of each file
- No integration test directories (`tests/` folders) were found

**Integration Tests:**
- Not implemented — no `crates/*/tests/` directories

**E2E Tests:**
- Not present in the repository

---

## Frontend Testing

**No test framework is configured.** The `package.json` in `frontend/` has no test script and no test dependencies (no `vitest`, `jest`, `@testing-library/react`, or similar).

No test files (`*.test.ts`, `*.spec.tsx`) exist in `frontend/src/`.

If tests are added, recommended approach given the stack (Vite + React 19):
- **Vitest** for unit/integration tests (compatible with Vite config)
- **React Testing Library** for component tests

---

*Testing analysis: 2026-03-22*
