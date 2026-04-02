# Orchestrator Policy Loop Design

**Goal**: Close the Phase 1 review gaps in the orchestrator policy loop without introducing a second orchestration path.

## Context

The existing daemon already has the core pieces needed for orchestrator intervention: awareness windows, counter-who repeated-approach detection, self-assessment, goal replanning, and escalation. The review blockers are integration bugs rather than missing architecture.

The broken behavior was:
- policy evaluation only ran after error results, so successful-but-stuck loops never triggered intervention
- retry guards only stopped repeats after another execution had already happened
- policy application used a fabricated trigger instead of the real evaluated context
- the policy module and test file had grown beyond the repository size guideline

## Scope

In scope:
- move policy evaluation to a post-tool checkpoint that runs for both error and non-error stuckness
- enforce retry guards before tool execution using the prospective approach hash
- thread the evaluated `PolicyTriggerContext` all the way into policy application
- split the oversized policy implementation and tests into focused submodules
- add coverage for the new loop behavior and document the design/plan artifacts

Out of scope:
- new policy families or a second orchestration runtime
- broader loop refactors unrelated to the Phase 1 blockers
- UI changes or operator workflow redesign

## Approach Options

### Option 1: Patch the existing loop checkpoints in place (recommended)

Reuse the current awareness, counter-who, replanning, and escalation paths. Move the decision points to the right places in `send_message_inner()` and keep policy application in the existing runtime helper.

Pros:
- smallest architectural change
- preserves existing policy memory and audit behavior
- directly fixes the review feedback with low risk

Cons:
- `agent_loop.rs` still coordinates several responsibilities

### Option 2: Build a separate orchestrator supervisor pass

Introduce a second control layer that watches tool execution externally and injects decisions back into the loop.

Pros:
- cleaner conceptual separation on paper

Cons:
- duplicates Phase 1 machinery
- higher integration risk
- explicitly conflicts with the review guidance to reuse existing paths

## Recommended Design

### Pre-execution guard checkpoint

Before any tool call is emitted or executed, derive a prospective approach hash from the tool name plus summarized arguments. If a live retry guard already exists for that exact approach within the current policy scope, halt immediately. This prevents the loop from paying the cost of one more identical attempt before the policy can act.

### Post-tool policy checkpoint

After every tool result is recorded into awareness, counter-who, and recent outcome history, build a policy evaluation context from the actual runtime signals. Trigger evaluation should run for both hard failures and successful-but-unproductive turns so low-progress loops can pivot even when no tool error occurred.

### Real trigger threading

`evaluate_triggers()` remains the single source of truth for intervention context. The resulting `PolicyTriggerContext` should be cloned from the evaluation context and passed through `apply_orchestrator_policy_decision()` unchanged. Pivot prompts and escalation decisions then reflect the actual signal mix that caused the intervention.

### Module split

Keep `orchestrator_policy.rs` as a small module entry point and move focused responsibilities into:
- `orchestrator_policy_types.rs`
- `orchestrator_policy_trigger.rs`
- `orchestrator_policy_memory.rs`
- `orchestrator_policy_prompt.rs`
- `orchestrator_policy_runtime.rs`

Split tests into `orchestrator_policy_tests/` by topic so each file stays under 500 LOC and failures stay localized.

## Files Affected

- `crates/amux-daemon/src/agent/agent_loop.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_types.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_trigger.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_memory.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_prompt.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_runtime.rs`
- `crates/amux-daemon/src/agent/orchestrator_policy_tests/`

## Testing Strategy

Add or update tests for:
- non-error stuckness causing policy evaluation and pivot on the real loop path
- pre-execution retry guard halting before guarded tool execution
- policy application consuming the actual evaluated trigger context
- split policy unit tests covering trigger selection, decision reuse, prompt generation, and runtime application

## Success Criteria

This design is successful when:
- non-error stuckness can trigger policy intervention
- a guarded retry is blocked before the tool executes
- policy application receives the evaluated trigger context, not a synthetic one
- policy code and tests comply with the 500 LOC file limit
- the targeted daemon tests pass
