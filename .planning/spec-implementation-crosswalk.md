# Spec Implementation Crosswalk

> Repository-grounded audit created 2026-04-11. This document separates architectural intent from current implementation status and is the source of truth for roadmap and wave-plan corrections.

## Status Legend
- `implemented`
- `partial`
- `planned-only`
- `superseded/by-existing-system`
- `unclear`

## Source Inventory Notes
- Canonical standalone spec documents currently present: 18
- Missing standalone canonical spec docs: Spec 18 — Causal Trace Reconstruction; Spec 19 — Contextual Tool Synthesis
- For Specs 18 and 19, classify from indirect planning references plus repository evidence instead of inventing replacement spec files during this audit.

## Entry Format
Each spec entry records:
- Wave
- Status
- Confidence
- Evidence
- Current Implementation Surface
- Remaining Gaps
- Planning Implication

## Spec 01 — Episodic → Semantic Memory Distillation
## Spec 02 — Trajectory-Informed Self-Reflection Loop
## Spec 03 — Probabilistic Agent Routing
## Spec 04 — Multi-Round Debate Protocol
## Spec 05 — Recursive Subagent Depth
## Spec 06 — Event-Driven Proactive Triggers
## Spec 07 — Dream State
## Spec 08 — Cognitive Resonance Engine
- Wave: 4
- Status: unclear
- Confidence: high

### Evidence
- Canonical-source conflict exists inside the planning corpus. `.planning/iteration-2/08-cognitive-resonance.md` defines Spec 08 as **real-time operator emotional intelligence**: a `cognitive_resonance/` module that infers operator cognitive state from revision velocity, session entropy, approval latency, and tool hesitation, then adapts verbosity, risk tolerance, proactiveness, and memory urgency.
- `.planning/wave-4-implementation-plan.md` instead defines Spec 08 as **cross-agent context sharing**: a `src/agent/resonance/mod.rs` context bus where Agent B can listen when Agent A retrieves context, with deduplication to avoid redundant LLM calls.
- The persona memory artifacts do not resolve that contradiction. `/home/mkurman/.tamux/agent/personas/domowoj/SOUL.md` and `/home/mkurman/.tamux/agent/personas/domowoj/MEMORY.md` describe shipped platform capabilities such as collaboration, implicit feedback learning, tool synthesis, provenance, and anticipatory runtime, but they do not define or claim a shipped Spec 08 subsystem.
- Repository evidence shows adjacent implementation for the **operator-model / implicit-feedback** interpretation: `crates/amux-daemon/src/agent/operator_model/model.rs` defines `ImplicitFeedback` and risk-fingerprint fields such as `fast_denials_by_category`, `auto_approve_categories`, and `auto_deny_categories`; `crates/amux-daemon/src/agent/operator_model/runtime.rs` implements `record_tool_hesitation(...)` and `learned_approval_decision(...)`; `crates/amux-daemon/src/agent/operator_model/metrics.rs` derives learned approval shortcuts; and `crates/amux-daemon/src/agent/tool_executor/managed_commands.rs` consumes that learned decision in managed-command approval flow.
- Repository evidence also shows adjacent implementation for the **cross-agent collaboration** interpretation: `crates/amux-daemon/src/agent/collaboration/runtime.rs` persists `CollaborationSession`s, exposes `collaboration_peer_memory_json(...)`, and manages disagreement voting / consensus; `crates/amux-daemon/src/agent/tool_executor/tasks.rs` wires `broadcast_contribution`, `read_peer_memory`, and `vote_on_disagreement`; `crates/amux-daemon/src/agent/tool_executor/catalog/part_d.rs` exposes those collaboration tools; and `docs/how-tamux-works.md` documents explicit collaboration sessions plus M9 implicit feedback learning.
- The audit did **not** find shipped code or schema matching either canonical Spec 08 design directly: no `cognitive_resonance` / `resonance` module, no `ResonanceScore` / `BehaviorAdjustment` / `CognitiveState` types, no `cognitive_resonance_samples` or `behavior_adjustments_log` tables, and no repository matches for a shared context bus / redundant-LLM-call deduplication subsystem.

### Current Implementation Surface
- tamux already has two neighboring capabilities that could be mistaken for Spec 08:
  - **Implicit-feedback/operator adaptation:** the operator model learns from fast denials, correction-style behavior, and tool hesitation, then uses those learned signals to auto-approve or auto-deny recurring command categories.
  - **Explicit collaboration state sharing:** subagents can publish contributions, read peer memory, inspect disagreements, and vote toward consensus inside persisted collaboration sessions.
- Those are real shipped systems, but they are separate subsystems with narrower purposes than either of the canonical Spec 08 definitions.
- `docs/how-tamux-works.md` currently presents implicit feedback as M9 and collaboration as a separate runtime capability, which reinforces that the shipped repository treats them as adjacent features rather than a unified “cognitive resonance engine.”

### Remaining Gaps
- **Canonical ambiguity remains unresolved:** the standalone spec and the wave plan disagree on whether Spec 08 is operator-state adaptation or cross-agent context resonance.
- If the standalone spec is canonical, the repository lacks the promised resonance-specific runtime: no cognitive-state model, no smoothing/resonance score pipeline, no prompt-parameter adaptation layer, and no dedicated persistence tables.
- If the wave-plan description is canonical, the repository still lacks the promised context-bus runtime: no shared context cache, no “agent listening” reuse path, and no deduplication layer aimed at avoiding redundant LLM calls across related agents.
- Existing operator-model and collaboration features provide partial ingredients, but there is no evidence they have been unified into one provenance-backed Spec 08 subsystem.

### Planning Implication
- Treat Spec 08 as **unclear-by-definition and unshipped as a standalone subsystem** until the planning corpus chooses one canonical meaning; after that decision, future work should either extend `operator_model/*` into explicit cognitive-state adaptation or extend `collaboration/*` into real shared-context resonance, rather than conflating the two.
## Spec 09 — Agent Morphogenesis
## Spec 10 — Capability Gene Pool
## Spec 11 — Temporal Foresight Engine
## Spec 12 — Consensus Architecture
## Spec 13 — Adversarial Self-Critique Loop
## Spec 14 — Emergent Protocol Negotiation
## Spec 15 — Meta-Cognitive Self-Model
## Spec 16 — Implicit Feedback Learning
## Spec 17 — Semantic Memory Palace
## Spec 18 — Causal Trace Reconstruction
## Spec 19 — Contextual Tool Synthesis
## Spec 20 — Intent Anticipation Engine
