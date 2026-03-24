# TAMUX Moat Architecture — Master Plan

**Status:** Draft  
**Author:** tamux agent (with operator)  
**Last Updated:** 2024

---

## Executive Summary

This document catalogs 10 strategic moats to differentiate tamux as a self-orchestrating agent that operates at a fundamentally different level than competitors. Each moat is analyzed for impact, effort, risk, and dependency ordering.

The unifying theme: **tamux as the agent that thinks with you, not just for you.**

---

## Current State Assessment

tamux already has a strong foundation:

| Layer | What's Built |
|-------|-------------|
| Execution | 56 focused Rust modules, LLM streaming, tool calls |
| Liveness | 4-layer checkpointing, health monitoring with hysteresis, 5 stuck patterns |
| Meta-Cognition | Self-assessment, 6 re-planning strategies, 4-level escalation |
| Learning | Execution traces, pattern mining, effectiveness tracking |
| Memory | 5-layer architecture (frozen, episodic, procedural, operational, Honcho) |
| Safety | Circuit breakers, rate limiting, approval gates |
| UI | Mission Control, CDUI, plugin system, goal runners |

**Structural advantage:** The daemon owns terminal, task queue, goal runners, agent loop, and memory in one process. This enables native wiring of every moat below.

---

## The 10 Moats

| # | Moat | Tagline | Phase |
|---|------|---------|-------|
| M1 | Operator Model | "The agent that knows how you think" | Phase 1 |
| M2 | Anticipatory Pre-loading | "The agent that acts before you ask" | Phase 1 |
| M3 | Causal Execution Traces | "Why did you do that?" | Phase 1 |
| M4 | Genetic Skill Evolution | "Skills that branch and compete" | Phase 2 |
| M5 | Semantic Environment Model | "The agent that understands your stack" | Phase 2 |
| M6 | Deep Storage Architecture | "The memory that knows what it knows" | Phase 2 |
| M7 | Multi-Agent Collaboration Protocol | "Agents that coordinate like peers" | Phase 3 |
| M8 | Trusted Execution Provenance | "The agent that can prove its work" | Phase 3 |
| M9 | Implicit Feedback Learning | "The agent that learns without being told" | Phase 2 |
| M10 | Runtime Tool Synthesis | "The agent that builds its own tools" | Phase 4 |

---

## Prioritization Matrix

| Moat | Impact | Effort | Risk | Do Phase |
|------|--------|--------|------|----------|
| M1: Operator Model | 🔴🔴🔴 | 🟡 Medium | 🟢 Low | **Phase 1** |
| M2: Anticipatory Pre-load | 🔴🔴🔴 | 🟡 Medium | 🟢 Low | **Phase 1** |
| M3: Causal Traces | 🔴🔴 | 🟡 Medium | 🟢 Low | **Phase 1** |
| M4: Genetic Skills | 🔴🔴 | 🔴 High | 🟡 Med | Phase 2 |
| M5: Semantic Env Model | 🔴🔴🔴 | 🔴 High | 🟡 Med | Phase 2 |
| M6: Deep Storage | 🔴🔴 | 🔴 High | 🟢 Low | Phase 2 (parallel) |
| M7: Multi-Agent Protocol | 🔴 | 🔴 High | 🔴 High | Phase 3 |
| M8: Trusted Provenance | 🔴 | 🔴 High | 🟡 Med | Phase 3 (enterprise) |
| M9: Implicit Feedback | 🔴🔴 | 🔴 High | 🔴 High | Phase 2 (experimental) |
| M10: Runtime Tool Synthesis | 🔴 | 🔴🔴 Very High | 🔴 High | Phase 4 |

---

## Dependency Graph

```
Phase 1 (Foundation)
├── M1: Operator Model
│   ├── Enables: M2 (session rhythm for pre-warm timing)
│   ├── Enables: M9 (behavioral learning loop)
│   └── Enables: M3 (operator-aware pattern mining)
├── M2: Anticipatory Pre-load
│   └── Depends on: M1 (session rhythm, attention state)
└── M3: Causal Traces
    └── Enables: M4 (causal skill variant success tracking)
                M5 (environment-aware decision history)

Phase 2 (Intelligence Layer)
├── M4: Genetic Skills
│   └── Depends on: M3 (success/failure attribution per variant)
├── M5: Semantic Env Model
│   └── Depends on: M3 (causal history of environment interactions)
├── M6: Deep Storage
│   └── Depends on: M3 (fact provenance)
│   └── Enables: M4, M5 (knowledge substrate)
└── M9: Implicit Feedback
    └── Depends on: M1 (operator model for behavioral inference)

Phase 3 (Advanced / Enterprise)
├── M7: Multi-Agent Protocol
│   └── Depends on: M1, M3, M4 (shared reasoning, conflict detection)
├── M8: Trusted Provenance
│   └── Depends on: M3 (signing causal trace events)

Phase 4 (Speculative)
└── M10: Runtime Tool Synthesis
    └── Depends on: M4, M5 (tool need detection, spec generation)
```

---

## Cross-Cutting Concerns

### All moats must satisfy:
1. **Opt-in by default**: No moat activates without operator consent
2. **Graceful degradation**: If a dependency fails, fall back to baseline behavior
3. **No silent failures**: Errors surface to operator, never swallow
4. **Auditability**: Every learning event is logged and reversible
5. **Operator override**: Any learned behavior can be explicitly corrected

### Infrastructure requirements:
- SQLite extension for graph edges (M6)
- Behavioral event bus for cross-layer signals
- Structured logging with correlation IDs
- Config flags for per-moat enable/disable

---

## Next Steps

1. Review this plan with operator
2. Prioritize Phase 1 moats (M1, M2, M3)
3. Write detailed specs for Phase 1 in `moats/phase-1/`
4. Identify first implementation sprint
