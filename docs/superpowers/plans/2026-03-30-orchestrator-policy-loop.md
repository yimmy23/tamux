# Orchestrator Policy Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to execute this plan in bounded steps.

**Goal:** Fix the Phase 1 orchestrator policy loop review blockers by moving intervention checkpoints to the correct runtime boundaries, preserving existing orchestration behavior, and adding the missing regression coverage.

**Architecture:** Reuse the current `amux-daemon` awareness, counter-who, replanning, and escalation paths. Strengthen the existing loop instead of adding a second supervisor layer.

**Tech Stack:** Rust, tokio test server helpers, existing `tamux-daemon` orchestrator policy runtime, `cargo test`

---

## File Structure

- Modify: `crates/amux-daemon/src/agent/agent_loop.rs`
- Modify: `crates/amux-daemon/src/agent/orchestrator_policy.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_types.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_trigger.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_memory.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_prompt.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_runtime.rs`
- Replace: `crates/amux-daemon/src/agent/orchestrator_policy_tests.rs`
- Add: `crates/amux-daemon/src/agent/orchestrator_policy_tests/`

## Task 1: Split the oversized policy module

- [ ] Move policy types, trigger evaluation, decision memory, prompt shaping, and runtime application into focused files.
- [ ] Keep `orchestrator_policy.rs` as a small module barrel that wires exports together.
- [ ] Split the old monolithic test file into topic-focused modules under `orchestrator_policy_tests/`.
- [ ] Verify each newly created policy source and test file stays under 500 LOC.

## Task 2: Enforce retry guards before execution

- [ ] In `agent_loop.rs`, compute the prospective approach hash from the tool name plus summarized arguments before emitting `AgentEvent::ToolCall`.
- [ ] Reuse the existing retry-guard scope and runtime enforcement helpers.
- [ ] Abort the loop immediately when the retry guard matches the prospective attempt.
- [ ] Add a real loop regression test showing no guarded tool call or tool result is emitted before the halt.

## Task 3: Evaluate policy on non-error stuckness

- [ ] Build a post-tool policy checkpoint that runs after awareness and episodic state have been updated.
- [ ] Ensure the checkpoint runs for both failing and successful tool results.
- [ ] Use the real runtime signals to trigger policy evaluation when awareness or repeated-approach signals indicate stuckness.
- [ ] Add a loop-level test through `send_message_inner()` that proves non-error stuckness reaches the pivot path.

## Task 4: Thread the evaluated trigger context through apply

- [ ] Preserve the `PolicyTriggerContext` returned by trigger evaluation inside the runtime context.
- [ ] Pass that same trigger into `apply_orchestrator_policy_decision()`.
- [ ] Remove synthetic trigger fabrication from the apply path.
- [ ] Add targeted coverage proving awareness-only trigger context changes the injected strategy refresh prompt.

## Task 5: Verify and finish

- [ ] Run targeted policy tests for the split module.
- [ ] Run the updated loop-level regression tests.
- [ ] Run a formatter if needed.
- [ ] Review `git diff` for scope drift.
- [ ] Commit with a conventional message describing the orchestrator policy loop fixes.

## Verification Commands

```bash
cargo test -p tamux-daemon orchestrator_policy -- --nocapture
cargo test -p tamux-daemon policy_halt_aborts_before_guarded_tool_execution_and_persists_failure_trace -- --nocapture
cargo test -p tamux-daemon post_tool_policy_checkpoint_pivots_for_non_error_stuckness_with_runtime_side_effect -- --nocapture
```

## Success Criteria

- retry guards stop identical repeated attempts before execution
- non-error stuckness can trigger policy intervention
- policy application receives the real evaluated trigger context
- split policy files remain under 500 LOC
- the targeted daemon tests pass and the changes are committed
