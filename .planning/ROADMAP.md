# Roadmap: tamux v3.0 -- The Intelligence Layer

## Overview

tamux v3.0 transforms the agent from a capable tool-user into a self-aware collaborator. The build progresses through four phases: first laying a memory foundation so the agent remembers what it tried and what failed, then adding awareness and judgment so it can sense when it is stuck and express honest confidence, then enabling structured delegation between specialist agents, and finally wiring operator-facing controls for cost visibility, autonomy tuning, and explainability. Every phase delivers observable intelligence gains -- the operator should feel the agent getting smarter at each boundary.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Memory Foundation** - Episodic memory store, counter-who self-model, and negative knowledge constraint graph
- [ ] **Phase 2: Awareness and Judgment** - Situational awareness, embodied metadata, and uncertainty quantification
- [ ] **Phase 3: Multi-Agent Orchestration** - Handoff broker, specialist profiles, context bundles, and divergent subagents
- [ ] **Phase 4: Operator Control and Transparency** - Cost accounting, autonomy dial, explainability mode, and shared authorship

## Phase Details

### Phase 1: Memory Foundation
**Goal**: The agent remembers what it tried, what failed, and what approaches are ruled out -- and uses that memory to avoid repeating mistakes
**Depends on**: Nothing (first phase)
**Requirements**: EPIS-01, EPIS-02, EPIS-03, EPIS-04, EPIS-05, EPIS-06, EPIS-07, EPIS-08, EPIS-09, EPIS-10, EPIS-11, CWHO-01, CWHO-02, CWHO-03, CWHO-04, NKNO-01, NKNO-02, NKNO-03, NKNO-04
**Success Criteria** (what must be TRUE):
  1. When a goal completes, the agent automatically records a structured episode (outcome, confidence, causal chain) and the episode is queryable within 100ms
  2. Before planning a new goal, the agent surfaces relevant past episodes (warnings for past failures, references for successes) and the operator can see them in the planning context
  3. The agent detects when it is re-attempting a previously failed approach and suggests a pivot instead of repeating the same pattern
  4. Ruled-out approaches are stored as negative knowledge constraints, consulted before planning, and expire after their TTL
  5. Operator can opt out of episode recording per-session, configure TTL, and PII is scrubbed from all stored episodes
**Plans**: 3 plans

Plans:
- [x] 01-01-PLAN.md -- Foundation: episodic module types, SQLite schema, basic CRUD, WORM, privacy, config
- [x] 01-02-PLAN.md -- FTS5 retrieval engine, goal boundary recording hooks, system prompt integration
- [x] 01-03-PLAN.md -- Counter-who self-model, negative knowledge constraint graph, agent loop wiring

### Phase 2: Awareness and Judgment
**Goal**: The agent senses when it is stuck, tracks the texture of its own activity, and expresses honest confidence grounded in structural evidence
**Depends on**: Phase 1
**Requirements**: AWAR-01, AWAR-02, AWAR-03, AWAR-04, AWAR-05, EMBD-01, EMBD-02, EMBD-03, EMBD-04, UNCR-01, UNCR-02, UNCR-03, UNCR-04, UNCR-05, UNCR-06, UNCR-07, UNCR-08
**Success Criteria** (what must be TRUE):
  1. When the agent repeats the same tool-call pattern N times without progress, it automatically shifts behavior and notifies the operator of diminishing returns
  2. Each goal plan step displays a confidence label (HIGH/MEDIUM/LOW) derived from structural signals (tool success rates, episodic familiarity, blast radius) -- not LLM self-assessment alone
  3. LOW-confidence actions in safety-critical domains block and require operator approval; LOW-confidence actions in research domains surface without blocking
  4. The operator can configure confidence thresholds and escalation behavior per domain in agent config
  5. Trajectory tracking (converging vs diverging from goal) is visible to the operator during active goal runs
**Plans**: 3 plans

Plans:
- [ ] 02-01-PLAN.md -- Situational awareness: per-entity failure tracking, sliding windows, trajectory, mode shifts with counter-who guard
- [ ] 02-02-PLAN.md -- Embodied metadata: 5 scalar dimensions (difficulty, familiarity, trajectory, temperature, weight)
- [ ] 02-03-PLAN.md -- Uncertainty quantification: structural confidence scoring, domain escalation, calibration, plan annotation, approval routing

### Phase 3: Multi-Agent Orchestration
**Goal**: The agent delegates tasks to specialist subagents with structured handoffs, validates their output, and can run divergent framings in parallel to surface productive disagreement
**Depends on**: Phase 2
**Requirements**: HAND-01, HAND-02, HAND-03, HAND-04, HAND-05, HAND-06, HAND-07, HAND-08, HAND-09, DIVR-01, DIVR-02, DIVR-03
**Success Criteria** (what must be TRUE):
  1. The agent can delegate a task to a specialist subagent matched by capability tags, with a context bundle that includes relevant episodes and partial outputs, and the specialist's output is validated against acceptance criteria before being accepted
  2. Handoff chains are limited to 3 hops and every handoff is logged to the WORM audit trail with full provenance (who, what, when, outcome, confidence)
  3. Default specialist profiles (researcher, backend-developer, frontend-developer, reviewer, generalist) work out of the box without operator configuration
  4. The agent can run divergent subagents in parallel on the same problem, and the operator sees the tensions and tradeoffs between framings rather than a forced consensus
**Plans**: TBD

Plans:
- [ ] 03-01: TBD
- [ ] 03-02: TBD

### Phase 4: Operator Control and Transparency
**Goal**: The operator has full visibility into cost, can tune agent autonomy per goal, can ask "why did you do that?" and get a real answer, and can see what the agent contributed vs what came from operator input
**Depends on**: Phase 1 (episodic data for explainability traces), Phase 2 (confidence data for autonomy decisions)
**Requirements**: AUTH-01, AUTH-02, COST-01, COST-02, COST-03, COST-04, AUTO-01, AUTO-02, AUTO-03, AUTO-04, EXPL-01, EXPL-02, EXPL-03
**Success Criteria** (what must be TRUE):
  1. The operator can see per-goal and per-session token counts and cost estimates, with budget alerts when spending exceeds a configured threshold
  2. The operator can set a per-goal autonomy level (autonomous / aware / supervised) and the agent's reporting behavior changes accordingly -- autonomous shows only the final report, supervised waits for acknowledgment at every significant step
  3. The operator can ask "why did you do that?" for any past action and receive a causal trace showing the decision point, chosen path, and rejected alternatives
  4. Significant agent outputs include attribution metadata showing what came from operator input, what from agent synthesis, and what is joint work
**Plans**: TBD

Plans:
- [ ] 04-01: TBD
- [ ] 04-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Memory Foundation | 3/3 | Complete | 2026-03-27 |
| 2. Awareness and Judgment | 0/3 | Planned | - |
| 3. Multi-Agent Orchestration | 0/2 | Not started | - |
| 4. Operator Control and Transparency | 0/2 | Not started | - |
