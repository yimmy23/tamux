# TAMUX Moats — Index

**Status:** Draft  
**Last Updated:** 2024

---

## Overview

This directory contains detailed specifications for the 10 strategic moats that differentiate tamux as a self-orchestrating agent.

**Unifying theme**: tamux as the agent that thinks with you, not just for you.

---

## Moat Inventory

| # | Moat | Phase | Status | File |
|---|------|-------|--------|------|
| M1 | Operator Model | 1 | Draft | `phase-1/M1-operator-model.md` |
| M2 | Anticipatory Pre-loading | 1 | Draft | `phase-1/M2-anticipatory-preloading.md` |
| M3 | Causal Execution Traces | 1 | Draft | `phase-1/M3-causal-execution-traces.md` |
| M4 | Genetic Skill Evolution | 2 | Draft | `phase-2/M4-M5-M6-M9-...md` |
| M5 | Semantic Environment Model | 2 | Draft | `phase-2/M4-M5-M6-M9-...md` |
| M6 | Deep Storage Architecture | 2 | Draft | `phase-2/M4-M5-M6-M9-...md` |
| M7 | Multi-Agent Collaboration Protocol | 3 | Draft | `phase-3/M7-M8-multi-agent-...md` |
| M8 | Trusted Execution Provenance | 3 | Draft | `phase-3/M7-M8-multi-agent-...md` |
| M9 | Implicit Feedback Learning | 2 | Draft | `phase-2/M4-M5-M6-M9-...md` |
| M10 | Runtime Tool Synthesis | 4 | Draft | `phase-4/M10-runtime-tool-synthesis.md` |

---

## Master Plan

See: `moat-architecture-master-plan.md`

---

## Quick Summary

### Phase 1 (Foundation) — High Impact, Low Risk
- **M1**: Knows how the operator thinks
- **M2**: Acts before you ask  
- **M3**: Remembers why it made decisions

### Phase 2 (Intelligence Layer) — High Impact, Medium Effort
- **M4**: Skills that evolve like code
- **M5**: Understands your environment
- **M6**: Memory that knows what it knows
- **M9**: Learns from behavior, not just explicit signals

### Phase 3 (Advanced) — Complex, Enterprise
- **M7**: Agents that collaborate like peers
- **M8**: Cryptographic audit trail

### Phase 4 (Speculative) — Major Capability
- **M10**: Builds its own tools at runtime

---

## Dependency Graph

```
Phase 1
├── M1 ──┬──→ M2 (pre-warm timing)
│        └──→ M9 (behavioral loop)
│
├── M2 ──── depends on M1
│
└── M3 ──┬──→ M4 (causal skill tracking)
         └──→ M5 (environment decisions)
             └──→ M10 (tool need detection)

Phase 2
├── M4 ──── depends on M3
├── M5 ──── depends on M3
├── M6 ──── depends on M3, enables M4/M5
└── M9 ──── depends on M1

Phase 3
├── M7 ──── depends on M1, M3, M4
└── M8 ──── depends on M3 (signing)

Phase 4
└── M10 ─── depends on M4, M5
```

---

## Cross-Cutting Concerns

All moats share these requirements:

1. **Opt-in by default** — No moat activates without operator consent
2. **Graceful degradation** — Fall back to baseline if dependency fails
3. **No silent failures** — Errors surface, never swallowed
4. **Auditability** — Every learning event is logged and reversible
5. **Operator override** — Any learned behavior can be corrected

---

## Current Infrastructure Needs

To support all moats:

| Need | Moats | Status |
|------|-------|--------|
| SQLite graph edges | M6 | Not implemented |
| Behavioral event bus | All | Not designed |
| Structured logging + correlation IDs | All | Partial |
| Per-moat config flags | All | Not designed |
| Privacy controls | M1, M9 | Not designed |

---

## Next Steps

1. Review master plan with operator
2. Prioritize Phase 1 implementation order
3. Begin M1 (Operator Model) — foundational for others
4. Build shared infrastructure (event bus, correlation IDs)
