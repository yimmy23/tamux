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
- Wave: 2
- Status: partial
- Confidence: high

### Evidence
- `.planning/iteration-2/13-adversarial-self-critique.md` defines Spec 13 as a dedicated `src/agent/critique/` subsystem with `advocate.rs`, `critic.rs`, `arbiter.rs`, explicit `CritiqueSession` / `Argument` / `Resolution` types, and persisted `critique_sessions`, `critique_arguments`, and `critique_resolutions` tables.
- Repository evidence now shows that subsystem is materially shipped. `crates/amux-daemon/src/agent/critique/mod.rs` implements `run_critique_preflight(...)`, `should_run_critique_preflight(...)`, `critique_requires_blocking_review(...)`, critique-session persistence, and retrieval of learned critique history; `crates/amux-daemon/src/agent/critique/advocate.rs`, `critic.rs`, `arbiter.rs`, and `types.rs` provide the explicit advocate / critic / arbiter roles and typed critique lifecycle the spec called for.
- Persistence is present rather than aspirational: `crates/amux-daemon/src/history/schema_sql_extra.rs` defines `critique_sessions`, `critique_arguments`, and `critique_resolutions`, while `crates/amux-daemon/src/history/critique.rs` persists and lists those records.
- Tool execution is now genuinely wrapped by critique for risky actions. `crates/amux-daemon/src/agent/tool_executor/execute_tool_impl.rs` runs critique preflight before suspicious / guard-always tools, applies typed or prose-derived safer modifications, can require operator confirmation for high-impact guarded actions, resumes approved critique continuations safely, and now supports two narrow executable fallback rewrites (`bash_command` → `replace_in_file`, `bash_command` → `apply_patch`) when critique guidance is already trivially mappable.
- The test surface is substantial. `crates/amux-daemon/src/agent/tool_executor/tests/part7.rs` covers critique-trigger conditions, learned-history influence, operator-confirmation gating and resume, end-to-end argument rewrites for shell / messaging / temporal / subagent / sensitive-file cases, and bounded executable fallback rewrites.

### Current Implementation Surface
- tamux now ships a real **adversarial self-critique layer** for risky or suspicious tool execution:
  - explicit advocate / critic / arbiter roles,
  - persisted critique sessions / arguments / resolutions,
  - risk-triggered preflight before selected tools,
  - operator-tolerance-aware proceed / modify / block handling,
  - automatic safer argument rewrites,
  - guarded operator confirmation and continuation replay,
  - learned critique history feeding future critique resolutions.
- This is no longer just “adjacent collaboration primitives.” It is an integrated critique subsystem that already influences live execution.

### Remaining Gaps
- Critique is still **selective**, not universal: it targets supported risky / suspicious tool classes rather than every action boundary.
- Executable fallback rewrites are currently narrow and typed (`replace_in_file`, `apply_patch`) rather than a broader shell-intent transformation system.
- The subsystem is grounded in daemon-native critique logic, not in full parallel multi-agent debate for every risky action.
- User-facing docs such as `docs/how-tamux-works.md` and related overview material do not yet fully reflect the now-shipped critique subsystem.

### Planning Implication
- Treat Spec 13 as **partially implemented with strong shipped foundations**. Future work should deepen trigger coverage, broaden safe executable rewrites cautiously, and improve operator-facing visibility/documentation rather than classifying the critique loop as missing.
## Spec 14 — Emergent Protocol Negotiation
- Wave: 5
- Status: planned-only
- Confidence: high

### Evidence
- `.planning/iteration-2/14-emergent-protocol-negotiation.md` defines Spec 14 as a new `src/agent/emergent_protocol/` subsystem with `pattern_detector.rs`, `compressor.rs`, `decoder.rs`, registry types like `ProtocolRegistryEntry`, and SQLite tables `emergent_protocols`, `protocol_steps`, and `protocol_usage_log`.
- `.planning/wave-5-implementation-plan.md` summarizes Spec 14 as agents inventing shorthand for repeated coordination patterns so they can communicate through compressed tokens instead of full instructions.
- Repository evidence shows a real explicit coordination stack already exists, but in a different form: `crates/amux-daemon/src/agent/tool_executor/catalog/part_c.rs` exposes `message_agent`, `handoff_thread_agent`, `run_divergent`, and `get_divergent_session`; `crates/amux-daemon/src/agent/thread_handoffs.rs` persists active-responder / handoff state; `crates/amux-daemon/src/agent/collaboration.rs` persists collaboration sessions; and `crates/amux-daemon/src/agent/handoff/divergent.rs` coordinates repeated structured collaboration via spawned framing tasks.
- The audit did **not** find the spec's emergent-protocol machinery in the shipped code: no `crates/amux-daemon/src/agent/emergent_protocol/` module, no protocol-registry schema in `crates/amux-daemon/src/history/schema_sql_extra.rs`, no mining of message history for recurring coordination sequences, and no runtime token decoder / fallback path for compressed inter-agent commands.
- Repository searches for the promised protocol tables and shorthand-token machinery returned matches only in planning docs, not in the daemon implementation.

### Current Implementation Surface
- Agents can already coordinate through **structured tools, persisted handoff state, and collaboration sessions**.
- That coordination is explicit, inspectable, and audit-friendly: messages, handoffs, disagreement votes, and divergent framings all remain in natural language or normal structured payloads.
- The system therefore has a solid coordination substrate, but it does **not** compress recurring workflows into learned shorthand languages.

### Remaining Gaps
- No pattern detector mining repeated coordination sequences from message history.
- No token generation / handshake / registry replication between agent pairs.
- No context-signature validation, success-rate tracking, fallback reasons, or garbage collection for learned protocols.
- No operator-facing decoding/audit surface for shorthand usage because shorthand protocols do not exist yet.

### Planning Implication
- Treat Spec 14 as **unimplemented**. If it remains desirable, future work should extend the existing handoff / collaboration / message-routing stack rather than adding a disconnected parallel coordination plane.
## Spec 15 — Meta-Cognitive Self-Model
- Wave: 2
- Status: partial
- Confidence: high

### Evidence
- `.planning/iteration-2/15-meta-cognitive-self-model.md` defines Spec 15 as a dedicated `meta_cognition/` subsystem with an introspector, pattern regulator, persistent self-model tables (`meta_cognition_model`, `cognitive_biases`, `workflow_profiles`), and explicit bias/calibration tracking.
- Repository evidence shows a real shipped meta-cognitive subsystem, but with a different shape: `crates/amux-daemon/src/agent/metacognitive/mod.rs` exposes `self_assessment`, `replanning`, `escalation`, and `resource_alloc`; `self_assessment.rs` defines `AssessmentInput`, `Assessment`, and `SelfAssessor`; `replanning.rs` selects recovery strategies such as `CompressRetry`, `SpawnExpert`, `UserGuidance`, and `GoalRevision`; and `escalation.rs` models multi-level escalation paths.
- This logic is not dormant. `crates/amux-daemon/src/agent/orchestrator_policy_runtime.rs` calls `replanning::select_replan_strategy(...)` and injects a strategy-refresh prompt, while `crates/amux-daemon/src/agent/orchestrator_policy_trigger.rs` treats `should_pivot` / `should_escalate` self-assessment signals as intervention triggers.
- The audit did **not** find the spec's promised persistent self-model store in the daemon schema: no `meta_cognition_model`, `cognitive_biases`, or `workflow_profiles` tables appear in `crates/amux-daemon/src/history/schema_sql_extra.rs`, and there is no shipped bias registry / calibration-offset model matching the standalone spec.

### Current Implementation Surface
- tamux already has a **working meta-cognitive control layer** for autonomy: self-assessment heuristics, re-planning strategy selection, escalation levels, and resource-allocation helpers are present and wired into orchestrator policy handling.
- The shipped system therefore does more than simple aspiration; it can recognize degraded execution and steer toward retries, pivots, subagents, user guidance, or escalation.
- However, the shipped implementation is centered on **runtime intervention and recovery**, not on a persistent, introspectable “self-image” of biases and calibration.

### Remaining Gaps
- No persistent self-model tables or versioned self-profile.
- No explicit cognitive-bias registry with trigger patterns / mitigation prompts.
- No historical confidence-calibration layer comparing predicted confidence vs. realized accuracy.
- No separate introspector/pattern-regulator pipeline running at every tool boundary in the way the spec describes.

### Planning Implication
- Treat Spec 15 as **partially implemented via the existing `metacognitive/*` and orchestrator-policy stack**. If the roadmap still wants the richer self-model from the standalone spec, it should extend the current subsystem with persistence and calibration rather than starting a parallel design.
## Spec 16 — Implicit Feedback Learning
- Wave: 5
- Status: partial
- Confidence: high

### Evidence
- `.planning/iteration-2/16-implicit-feedback-learning.md` defines Spec 16 as a dedicated `implicit_feedback/` subsystem with signal extraction, satisfaction scoring, behavior adaptation, and new SQLite tables `implicit_signals` plus `satisfaction_scores`.
- Repository evidence shows a shipped adjacent implementation inside the operator-model stack rather than in a standalone `implicit_feedback/` module. `crates/amux-daemon/src/agent/operator_model/model.rs` defines `ImplicitFeedback`, `AttentionTopology`, and `RiskFingerprint` fields such as `tool_hesitation_count`, `revision_message_count`, `fast_denial_count`, `rapid_revert_count`, `fallback_histogram`, `top_tool_fallbacks`, `auto_approve_categories`, and `auto_deny_categories`.
- `crates/amux-daemon/src/agent/operator_model/runtime.rs` actively records those signals through `record_operator_message(...)`, `record_tool_hesitation(...)`, `record_attention_surface(...)`, `record_operator_approval_resolution(...)`, and `record_rapid_revert_feedback(...)`; it also renders them back into the prompt via `build_operator_model_prompt_summary(...)`.
- File-mutation provenance now feeds implicit feedback directly. `crates/amux-daemon/src/agent/work_context.rs` captures agent-authored file edits, refreshes repo state, and detects rapid reverts within a bounded window before persisting a thread-scoped `rapid_revert` signal.
- The learned signals are not prompt-only. `crates/amux-daemon/src/agent/operator_model/runtime.rs` exposes `learned_approval_decision(...)`, and `crates/amux-daemon/src/agent/tool_executor/managed_commands.rs` consumes that result to auto-approve or auto-deny managed commands based on learned operator patterns.
- The SQLite persistence layer from the spec is also present in the shipped code: `crates/amux-daemon/src/history/schema_sql_extra.rs` creates `implicit_signals` and `satisfaction_scores`, and `crates/amux-daemon/src/history/implicit_feedback.rs` persists / lists both tables.

### Current Implementation Surface
- tamux already learns from several implicit signals: revision-style operator messages, tool-fallback / hesitation patterns, fast denials, short-dwell / attention-churn telemetry, and rapid file reverts after agent-authored edits.
- Those signals are persisted in the operator model and affect behavior in at least two real ways:
  - **Prompt shaping:** the agent sees a learned operator-model summary including fallback / revision / rapid-revert signals.
  - **Approval behavior:** repeated approval history can produce learned auto-approve / auto-deny shortcuts.
- This is therefore a real silent-feedback loop, but it is embedded in `operator_model/*` rather than packaged as the planned satisfaction-model subsystem.

### Remaining Gaps
- No standalone `implicit_feedback/` module; the capability lives inside `operator_model/*`.
- No robust session-abandonment detector matching the standalone spec.
- The dwell-time signal is approximated via attention churn / short-dwell heuristics rather than a richer per-output dwell model.
- No generalized behavior adapter that continuously tunes verbosity, clarification frequency, and tool strategy from a unified score.

### Planning Implication
- Treat Spec 16 as **partially implemented and already product-relevant**. Future work should deepen `operator_model/*` into a fuller satisfaction / adaptation system instead of building a disconnected `implicit_feedback/` subsystem from scratch.
## Spec 17 — Semantic Memory Palace
- Wave: 3
- Status: partial
- Confidence: high

### Evidence
- `.planning/iteration-2/17-semantic-memory-palace.md` and `.planning/wave-3-implementation-plan.md` define Spec 17 as a persistent knowledge graph with `memory_palace/` modules, graph-builder / navigator / pruner components, and graph tables such as `memory_nodes`, `memory_edges`, and cluster tables.
- Repository evidence shows two substantial adjacent shipped surfaces:
  - `crates/amux-daemon/src/agent/semantic_env/mod.rs` implements `semantic_query` over workspace packages, services, infrastructure, imports, conventions, and temporal history.
  - `crates/amux-daemon/src/agent/context/structural_memory.rs` defines persisted per-thread structural state (`ThreadStructuralMemory`) with `workspace_seeds`, `observed_files`, and typed `edges`; it enriches that graph-like state from file-tool results and import detection, and the schema persists it in `thread_structural_memory` inside `crates/amux-daemon/src/history/schema_sql_extra.rs`.
- `crates/amux-daemon/src/agent/work_context.rs` and `anticipatory.rs` actively refresh thread repo context, so the structural memory is used operationally rather than being dead data.
- The audit did **not** find the planned `memory_palace/` subsystem or the dedicated long-term graph tables (`memory_nodes`, `memory_edges`, `memory_graph_clusters`, `memory_cluster_members`).

### Current Implementation Surface
- tamux already has a **semantic workspace layer** and a **graph-like per-thread structural memory**.
- The shipped system can answer semantic questions about packages, imports, services, conventions, and temporal workspace history, and it can retain structural file / import relationships tied to a thread.
- This provides meaningful semantic context retrieval, but it is narrower than the planned long-horizon “memory palace” knowledge graph spanning files, errors, tasks, and abstract concepts.

### Remaining Gaps
- No global `memory_nodes` / `memory_edges` knowledge graph.
- No builder that continuously extracts entities and relations from execution traces into a persistent cross-thread graph.
- No explicit graph navigator / pruner pipeline with clustering, decay, or summary-node compression.
- No evidence of graph retrieval over concepts/errors/tasks in the broad GraphRAG-style sense the standalone spec describes.

### Planning Implication
- Treat Spec 17 as **partially implemented through `semantic_env/*` plus thread structural memory**. If the full memory-palace roadmap remains desirable, it should grow out of those shipped semantic/structural layers rather than ignoring them.
## Spec 18 — Causal Trace Reconstruction
- Wave: 1
- Status: implemented
- Confidence: high

### Evidence
- There is no standalone canonical spec file for Spec 18 in the planning corpus, but `.planning/wave-1-implementation-plan.md` clearly defines it as causal trace recording for “why” behind agent actions.
- Repository evidence shows a full shipped subsystem for this capability: `crates/amux-daemon/src/agent/learning/traces.rs` defines `CausalTrace`, `DecisionOption`, `CausalFactor`, and `CausalTraceOutcome`; `crates/amux-daemon/src/history/schema_sql_extra.rs` creates the `causal_traces` table; and `crates/amux-daemon/src/history/causal_traces.rs` persists, lists, and settles trace records.
- The agent layer actively uses that storage. `crates/amux-daemon/src/agent/causal_traces.rs` records and settles causal traces for skill selection / goal planning, `agent/causal_traces/reporting.rs` produces causal-trace and counterfactual reports plus “Recent Causal Guidance,” and `crates/amux-daemon/src/agent/explainability.rs` uses causal traces as the first source for “Why did you do that?” explanations.
- The server/API surface exposes it as a first-class feature: `crates/amux-daemon/src/server/dispatch_part6.rs` handles `AgentGetCausalTraceReport` and returns causal-trace reports to clients.

### Current Implementation Surface
- tamux ships a real **decision-trace and explainability pipeline**:
  - decision-level causal traces with selected and rejected options,
  - causal factors and settled outcomes,
  - reporting / counterfactual summaries,
  - explainability queries grounded in stored traces,
  - server exposure for client access.
- This is not merely scaffolding; it is already integrated into explanation, guidance, and historical reporting flows.

### Remaining Gaps
- The early Wave 1 wording implied a very fine-grained step-predecessor linkage model; the shipped implementation is strongest at **decision-level traceability** rather than a universal tool-input/output DAG for every action.
- That is a refinement opportunity, not grounds to call the feature absent.

### Planning Implication
- Treat Spec 18 as **implemented**. Future work should refine granularity or UX around the existing causal-trace system rather than classifying it as missing.
## Spec 19 — Contextual Tool Synthesis
- Wave: 4
- Status: partial
- Confidence: high

### Evidence
- There is no standalone canonical spec file for Spec 19, but `.planning/wave-4-implementation-plan.md` defines it as generating missing tools on the fly, sandboxing them, and registering them for use.
- Repository evidence shows a substantial shipped implementation. `crates/amux-daemon/src/agent/tool_synthesis_runtime.rs` implements:
  - `synthesize_cli_tool(...)` from conservative CLI `--help` surfaces,
  - `synthesize_openapi_tool(...)` from GET OpenAPI operations,
  - guarded runtime execution for generated tools,
  - activation / promotion / retirement helpers,
  - safety checks like `validate_safe_cli_invocation(...)` plus sandbox caps.
- The live tool catalog already exposes this surface: `crates/amux-daemon/src/agent/tool_executor/catalog/part_d.rs` includes `synthesize_tool`, `list_generated_tools`, `promote_generated_tool`, and `activate_generated_tool`.
- `crates/amux-daemon/src/server/dispatch_part6.rs` handles async synthesize-tool operations, and the repository includes focused synthesis tests (`agent/tests/tool_synthesis.rs`, `server/tests_part2_synthesize_divergent.rs`, `server/tests_part2_agent_work_skills.rs`).

### Current Implementation Surface
- tamux can already synthesize **guarded generated tools** at runtime, register them in the local generated-tool registry, execute them under safety limits, and later activate or promote them when useful.
- This is a genuine shipped tool-synthesis system, not a placeholder.
- However, the shipped implementation is deliberately conservative: it specializes in safe wrapper generation around existing CLI/OpenAPI surfaces rather than arbitrary new-code synthesis.

### Remaining Gaps
- No automatic gap detector that notices a missing capability during planning and synthesizes a tool without explicit request.
- No general-purpose Python/Rust tool-code generator of the kind the wave plan sketches.
- Narrow synthesis scope: conservative CLI wrappers and GET OpenAPI operations only.
- Review / testing exist, but not as the broader autonomous “detect gap → write tool → sandbox test → self-register” loop described in the plan.

### Planning Implication
- Treat Spec 19 as **partially implemented with strong shipped foundations**. Future work should add automatic gap detection and broader synthesis sources on top of the current generated-tool runtime rather than replacing it.
## Spec 20 — Intent Anticipation Engine
- Wave: 4
- Status: partial
- Confidence: high

### Evidence
- `.planning/iteration-2/20-intent-anticipation-engine.md` and `.planning/wave-4-implementation-plan.md` define Spec 20 as explicit next-action prediction plus pre-execution / cached speculative results.
- Repository evidence shows a shipped anticipatory subsystem under a different name. `crates/amux-daemon/src/agent/anticipatory.rs` implements `run_anticipatory_tick()`, operator-attention tracking, session-start prewarm, predictive hydration, morning briefs, stuck hints, and collaboration hints; `anticipatory_support.rs` provides routing / surfacing helpers; and `tests/anticipatory.rs` covers these behaviors.
- The runtime is active in server flow: repository search shows `crates/amux-daemon/src/server/dispatch_part4.rs` calling `agent.run_anticipatory_tick().await` and `agent.emit_anticipatory_snapshot().await`.
- The anticipatory runtime is tied to real context hydration. `anticipatory.rs` calls `refresh_thread_repo_context(...)`, which updates thread repo context in `crates/amux-daemon/src/agent/work_context.rs`.
- The audit did **not** find the specific `intent_engine/` module from the planning docs, nor a ranked prediction model that outputs explicit probable next actions with cached speculative execution results.

### Current Implementation Surface
- tamux already has a real **anticipatory runtime**:
  - proactive morning/stuck/collaboration hints,
  - operator-attention-aware routing,
  - session reconnect prewarming,
  - predictive hydration of active thread repo context.
- This means the system is already proactive in a meaningful way; it does work ahead of time to reduce friction and surface likely-needing context.
- But the shipped implementation is focused on **context prewarming and hint surfacing**, not on a general next-action prediction engine with speculative cached tool results.

### Remaining Gaps
- No explicit `IntentPrediction` / `Opportunity` model matching the standalone spec.
- No ranked “operator will probably do X next” action predictions.
- No broad speculative execution cache returning precomputed tool results on demand.
- No dedicated `intent_models` / `intent_predictions` schema as described in the standalone spec.

### Planning Implication
- Treat Spec 20 as **partially implemented through the existing anticipatory runtime**. If the roadmap still wants full intent prediction and speculative execution, it should evolve `anticipatory/*` into that richer engine rather than pretending there is no anticipatory substrate today.
