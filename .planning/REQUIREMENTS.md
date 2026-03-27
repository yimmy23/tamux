# Requirements: tamux v3.0 -- The Intelligence Layer

**Defined:** 2026-03-26
**Core Value:** An agent that knows what it knows, remembers what it tried, and gets smarter from every interaction

## v1 Requirements

Requirements for v3.0 release. Each maps to roadmap phases.

### Episodic Memory

- [ ] **EPIS-01**: Agent stores structured episode records automatically on goal start and completion (goal_text, goal_type, outcome, confidence_before/after, summary)
- [x] **EPIS-02**: Agent stores causal chain data linking failures to root causes (step -> cause -> effect chains)
- [x] **EPIS-03**: Agent proactively surfaces top 5 relevant past episodes before planning similar goals (WARNING for failures, CAUTION for partial, REFERENCE for success)
- [x] **EPIS-04**: FTS5 episodic index supports temporal retrieval ("what happened in the last N sessions?")
- [x] **EPIS-05**: FTS5 episodic index supports entity-aware retrieval ("all goals that touched this file/service")
- [x] **EPIS-06**: FTS5 episodic index supports causal retrieval ("what failed last time we tried this approach?")
- [ ] **EPIS-07**: Episode links connect related goals (retry_of, builds_on, contradicts, supersedes)
- [ ] **EPIS-08**: Session headers with auto-generated summary and tags on session end
- [ ] **EPIS-09**: Privacy controls: operator opt-out flag, per-session suppression, configurable TTL (default 90 days), PII scrubbing via scrub_sensitive
- [ ] **EPIS-10**: Retrieval has hard cap (max 5 episodes, max token budget per injection) to prevent context pollution
- [ ] **EPIS-11**: Episodes are WORM -- append-only, never edited. Corrections are new episodes referencing old ones

### Counter-Who (Persistent Self-Model)

- [x] **CWHO-01**: Background self-model tracks what the agent is doing, what's changed, and what's been tried across turns
- [x] **CWHO-02**: Counter-who detects repeated approaches ("we tried 3 variants of this in the last hour, all failed") and suggests pivots
- [x] **CWHO-03**: Counter-who tracks operator corrections and flags persistent patterns ("operator corrected me twice on the same thing")
- [x] **CWHO-04**: Counter-who state persists across session within a goal run, rehydrates from episodic store on restart

### Negative Knowledge (Constraint Graph)

- [x] **NKNO-01**: Constraint graph stores ruled-out approaches with reasons (dead, dying, impossible, suspicious)
- [x] **NKNO-02**: Constraint graph entries include the class of solutions eliminated ("approach A failed -> all approaches depending on assumption Z are eliminated")
- [x] **NKNO-03**: Agent consults constraint graph before planning, avoids ruled-out approaches
- [x] **NKNO-04**: Constraint entries have TTL-based expiry (default 30 days) to prevent stale constraints blocking valid approaches

### Multi-Agent Handoffs

- [ ] **HAND-01**: HandoffBroker matches tasks to specialist profiles by capability tags (proficiency levels: expert, advanced, competent, familiar)
- [ ] **HAND-02**: Context bundles carry typed references: memory refs, episodic refs, document refs, partial outputs
- [ ] **HAND-03**: Context bundles are summarized with strict token ceiling (not forwarded raw) to prevent exponential growth
- [ ] **HAND-04**: Escalation chains with structured triggers (ConfidenceBelow, ToolFails, TimeExceeds) and actions (HandBack, RetryWithNewContext, EscalateTo, AbortWithReport)
- [ ] **HAND-05**: Orchestrator validates specialist output against acceptance criteria before accepting
- [ ] **HAND-06**: Every handoff logged to WORM audit trail (from, to, task, outcome, duration, confidence, audit_hash)
- [ ] **HAND-07**: Default specialist profiles ship out of the box (researcher, backend-developer, frontend-developer, reviewer, generalist)
- [ ] **HAND-08**: Handoff depth limit (max 3 hops) to prevent handoff loops, then escalate to operator
- [ ] **HAND-09**: HandoffBroker layers on existing spawn_subagent primitive (not a separate orchestration engine)

### Divergent Subagents

- [ ] **DIVR-01**: Parallel interpretation mode where multiple framings work the same problem simultaneously
- [ ] **DIVR-02**: Disagreement between framings is surfaced as the valuable output (tensions, not consensus)
- [ ] **DIVR-03**: Mediator synthesizes tensions into a recommendation that acknowledges tradeoffs

### Uncertainty Quantification

- [ ] **UNCR-01**: Planning confidence: each goal plan step rated HIGH/MEDIUM/LOW with evidence and dissent
- [ ] **UNCR-02**: Tool-call confidence: pre-execution warnings with blast-radius uncertainty before policy-flagged commands
- [ ] **UNCR-03**: Output confidence: research results labeled by source authority (official/community/unknown) and freshness
- [ ] **UNCR-04**: Domain-specific escalation: Safety/Reliability domains block on LOW, Research/Business surface without blocking
- [ ] **UNCR-05**: Operator preferences: configurable thresholds per domain in agent config
- [ ] **UNCR-06**: Confidence derives from hybrid signals (structural: tool success rates, episodic familiarity, blast radius + verbal LLM self-assessment), not LLM alone
- [ ] **UNCR-07**: Calibration feedback loop: operator corrections adjust confidence model over time
- [ ] **UNCR-08**: If all plan steps are HIGH -> proceed autonomously. Any MEDIUM -> inform operator. Any LOW -> require approval.

### Embodied Metadata

- [ ] **EMBD-01**: Scalar dimensions tracked per action: difficulty (retries, error rate), familiarity (pattern match to prior sessions), trajectory (converging/diverging from goal)
- [ ] **EMBD-02**: Temperature dimension: urgency signals from operator messages
- [ ] **EMBD-03**: Weight dimension: conceptual mass distinguishing light suggestions from heavy architectural commitments
- [ ] **EMBD-04**: Embodied metadata feeds into uncertainty scoring (unfamiliar + high difficulty -> lower confidence)

### Situational Awareness (Frustration Proxy)

- [x] **AWAR-01**: Empirical failure tracking across all agent activity (tool calls, sessions, browsing, goal runs) -- not scoped to goal runner only
- [x] **AWAR-02**: Automatic mode shift when diminishing returns detected (same pattern N times with no progress)
- [x] **AWAR-03**: Counter-who is consulted before mode shifts fire (prevents false positives from repetitive-but-productive reasoning)
- [x] **AWAR-04**: Trajectory tracking: converging vs diverging from goal, surfaced to operator and available to confidence scoring
- [x] **AWAR-05**: Sliding window analysis (short-term: last 5 actions, medium-term: last 30 minutes, long-term: session)

### Shared Authorship

- [ ] **AUTH-01**: Significant outputs attribute contributions: what came from operator input, what from agent synthesis, what's joint
- [ ] **AUTH-02**: Attribution is metadata on the output, not inline commentary (not disruptive to reading flow)

### Cost & Token Accounting

- [ ] **COST-01**: Per-goal token counts (prompt + completion) tracked on every LLM API call
- [ ] **COST-02**: Per-session and cumulative cost estimates using provider rate cards
- [ ] **COST-03**: Budget alerts when spending exceeds operator-defined threshold
- [ ] **COST-04**: Cost data persisted in goal_run metadata, queryable via observability

### Autonomy Dial

- [ ] **AUTO-01**: Per-goal autonomy level setting: autonomous / aware / supervised
- [ ] **AUTO-02**: Autonomous: agent proceeds, operator sees final report only
- [ ] **AUTO-03**: Aware: agent reports on milestones (sub-task completions, handoffs)
- [ ] **AUTO-04**: Supervised: agent reports on every significant step and waits for acknowledgment

### Explainability Mode

- [ ] **EXPL-01**: On-demand reasoning log: why agent chose Plan A over Plan B with rejected alternatives
- [ ] **EXPL-02**: "Why did you do that?" query returns causal trace for any past action
- [ ] **EXPL-03**: Rejected alternatives and decision points stored alongside chosen plan in goal_run metadata

## v2 Requirements

Deferred to v3.1+. Tracked but not in current roadmap.

### Advanced Memory

- **AMEM-01**: Vector embeddings for semantic similarity retrieval (sqlite-vec or similar)
- **AMEM-02**: Cross-operator episodic memory for team settings
- **AMEM-03**: Episode usefulness voting by operator (feedback to tune retrieval ranking)

### Advanced Planning

- **APLN-01**: Tree-of-Thoughts branching exploration (builds on episodic memory foundation)
- **APLN-02**: Confidence-adaptive token budgets (uncertain paths get more reasoning tokens)

### Advanced Orchestration

- **AORC-01**: Async handoff task queue (specialist doesn't need to be online)
- **AORC-02**: Custom specialist profiles defined by operator
- **AORC-03**: Shared semantic memory layer between specialists

### Skills

- **ASKL-01**: Skill versioning with change tracking
- **ASKL-02**: Skills marketplace and import from central registry

## Out of Scope

| Feature | Reason |
|---------|--------|
| Structured data operations (CSV/JSON/Parquet) | Already possible via bash/python tool calls. Not a core capability gap. |
| Computer use / screenshot action | High effort, narrow use case. Browser MCP via Lightpanda is the right approach. |
| RAG pipelines | FTS5 + episodic memory covers retrieval needs without adding vector DB complexity. |
| State graphs (LangGraph-style) | Goal runners + handoff broker cover orchestration needs. |
| External agent framework adoption | tamux already has its own agent loop, tool executor, and goal runners. Adding AutoGen/CrewAI/LangGraph would be an architectural mismatch. |
| Numeric confidence scores as default display | LLMs are miscalibrated on numeric confidence. Labels (HIGH/MEDIUM/LOW) are actionable. Numeric scores available as optional instrumentation. |
| Level 4 full autonomy | Intentional design decision -- always maintain operator oversight. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| EPIS-01 | Phase 1 | Pending |
| EPIS-02 | Phase 1 | Complete |
| EPIS-03 | Phase 1 | Complete |
| EPIS-04 | Phase 1 | Complete |
| EPIS-05 | Phase 1 | Complete |
| EPIS-06 | Phase 1 | Complete |
| EPIS-07 | Phase 1 | Pending |
| EPIS-08 | Phase 1 | Pending |
| EPIS-09 | Phase 1 | Pending |
| EPIS-10 | Phase 1 | Pending |
| EPIS-11 | Phase 1 | Pending |
| CWHO-01 | Phase 1 | Complete |
| CWHO-02 | Phase 1 | Complete |
| CWHO-03 | Phase 1 | Complete |
| CWHO-04 | Phase 1 | Complete |
| NKNO-01 | Phase 1 | Complete |
| NKNO-02 | Phase 1 | Complete |
| NKNO-03 | Phase 1 | Complete |
| NKNO-04 | Phase 1 | Complete |
| AWAR-01 | Phase 2 | Complete |
| AWAR-02 | Phase 2 | Complete |
| AWAR-03 | Phase 2 | Complete |
| AWAR-04 | Phase 2 | Complete |
| AWAR-05 | Phase 2 | Complete |
| EMBD-01 | Phase 2 | Pending |
| EMBD-02 | Phase 2 | Pending |
| EMBD-03 | Phase 2 | Pending |
| EMBD-04 | Phase 2 | Pending |
| UNCR-01 | Phase 2 | Pending |
| UNCR-02 | Phase 4 | Pending |
| UNCR-03 | Phase 4 | Pending |
| UNCR-04 | Phase 2 | Pending |
| UNCR-05 | Phase 2 | Pending |
| UNCR-06 | Phase 2 | Pending |
| UNCR-07 | Phase 2 | Pending |
| UNCR-08 | Phase 2 | Pending |
| HAND-01 | Phase 3 | Pending |
| HAND-02 | Phase 3 | Pending |
| HAND-03 | Phase 3 | Pending |
| HAND-04 | Phase 3 | Pending |
| HAND-05 | Phase 3 | Pending |
| HAND-06 | Phase 3 | Pending |
| HAND-07 | Phase 3 | Pending |
| HAND-08 | Phase 3 | Pending |
| HAND-09 | Phase 3 | Pending |
| DIVR-01 | Phase 3 | Pending |
| DIVR-02 | Phase 3 | Pending |
| DIVR-03 | Phase 3 | Pending |
| AUTH-01 | Phase 4 | Pending |
| AUTH-02 | Phase 4 | Pending |
| COST-01 | Phase 4 | Pending |
| COST-02 | Phase 4 | Pending |
| COST-03 | Phase 4 | Pending |
| COST-04 | Phase 4 | Pending |
| AUTO-01 | Phase 4 | Pending |
| AUTO-02 | Phase 4 | Pending |
| AUTO-03 | Phase 4 | Pending |
| AUTO-04 | Phase 4 | Pending |
| EXPL-01 | Phase 4 | Pending |
| EXPL-02 | Phase 4 | Pending |
| EXPL-03 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 61 total
- Mapped to phases: 61
- Unmapped: 0

---
*Requirements defined: 2026-03-26*
*Last updated: 2026-03-26 after roadmap creation (compressed 5-phase to 4-phase structure)*
