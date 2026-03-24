# TAMUX Moat Cheatsheet

## The 10 Moats at a Glance

```
┌─────────────────────────────────────────────────────────────────────────┐
│  TAMUX MOATS — "The agent that thinks with you"                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  PHASE 1: FOUNDATION                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ M1 🧠 Operator Model       — Knows how YOU think                  │  │
│  │     ├─ Cognitive style, risk tolerance, session rhythm            │  │
│  │     └─ Enables: M2, M9                                             │  │
│  │                                                                     │  │
│  │ M2 ⚡ Anticipatory Pre-load — Acts BEFORE you ask                 │  │
│  │     ├─ Morning brief, predictive hydration, stuck detection      │  │
│  │     └─ Depends on: M1                                             │  │
│  │                                                                     │  │
│  │ M3 🔮 Causal Traces       — Knows WHY it made decisions          │  │
│  │     ├─ Decision attribution, counterfactual reasoning            │  │
│  │     └─ Enables: M4, M5, M8                                        │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  PHASE 2: INTELLIGENCE LAYER                                           │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ M4 🧬 Genetic Skills      — Skills evolve like code               │  │
│  │     ├─ Branching, competition, pruning, merging                 │  │
│  │     └─ Depends on: M3                                             │  │
│  │                                                                     │  │
│  │ M5 🏗️  Semantic Env Model  — Understands YOUR stack               │  │
│  │     ├─ Dependency graph, conventions, temporal context         │  │
│  │     └─ Depends on: M3                                             │  │
│  │                                                                     │  │
│  │ M6 🌊 Deep Storage         — Memory that knows what it knows     │  │
│  │     ├─ Provenance, contradiction detection, temporal decay      │  │
│  │     └─ Substrate for M4, M5                                       │  │
│  │                                                                     │  │
│  │ M9 🧬 Implicit Feedback    — Learns from behavior                │  │
│  │     ├─ Tool hesitation, revision patterns, approval bypass      │  │
│  │     └─ Depends on: M1                                             │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  PHASE 3: ADVANCED / ENTERPRISE                                        │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ M7 🔄 Multi-Agent Protocol — Agents coordinate like peers         │  │
│  │     ├─ Shared context, conflict detection, voting                │  │
│  │     └─ Depends on: M1, M3, M4                                    │  │
│  │                                                                     │  │
│  │ M8 🛡️  Trusted Provenance  — Cryptographic audit trail           │  │
│  │     ├─ Hash chain, signatures, SOC2 artifacts                   │  │
│  │     └─ Depends on: M3                                             │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  PHASE 4: SPECULATIVE                                                  │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ M10 🏗️ Runtime Tool Synthesis — Builds its own tools             │  │
│  │     ├─ CLI/API introspect, sandboxed deploy, promotion          │  │
│  │     └─ Depends on: M4, M5                                        │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│  IMPACT × EFFORT × RISK                                                │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  🔴🔴🔴  High Impact  │  🟡 Medium Effort  │  🟢 Low Risk        │  │
│  │  ───────────────────────────────────────────────────────────────│  │
│  │  M1, M2, M5          │  M1, M2, M3        │  M1, M2, M3, M6     │  │
│  │  M6                  │                     │                     │  │
│  │                                                                      │  │
│  │  🔴🔴  Medium Impact │  🔴 High Effort    │  🟡 Medium Risk      │  │
│  │  ───────────────────────────────────────────────────────────────│  │
│  │  M3, M4, M9          │  M4, M5, M6, M7,  │  M4, M5, M8         │  │
│  │                      │  M8, M9             │                     │  │
│  │                                                                      │  │
│  │  🔴  Lower Impact   │  🔴🔴 Very High    │  🔴 High Risk        │  │
│  │  ───────────────────────────────────────────────────────────────│  │
│  │  M7, M8, M10         │  M10               │  M7, M9, M10         │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Quick Implementation Order

```
START HERE ──────────────────────────────────────────────────────────
 │
 ├─ 1. M3 (Causal Traces)
 │      └─ Everything else uses causal history
 │
 ├─ 2. M1 (Operator Model)
 │      └─ M2 depends on it; enables all learning
 │
 └─ 3. M2 (Anticipatory Pre-load)
         └─ Biggest UX moat, low risk

THEN ─────────────────────────────────────────────────────────────────
 │
 ├─ M6 (Deep Storage) — Memory foundation
 ├─ M4 (Genetic Skills) — Procedural memory evolution  
 └─ M5 (Semantic Environment) — Environment understanding

LATER ────────────────────────────────────────────────────────────────
 │
 ├─ M9 (Implicit Feedback) — Refine M1
 ├─ M7 (Multi-Agent) — Only if scope warrants
 └─ M8 (Trusted Provenance) — Enterprise track

MAYBE ────────────────────────────────────────────────────────────────
 │
 └─ M10 (Runtime Tool Synthesis) — Major effort, high risk
```

## Key Files

| File | Purpose |
|------|---------|
| `moats/README.md` | This index |
| `moats/moat-architecture-master-plan.md` | Full master plan |
| `moats/phase-1/M1-operator-model.md` | M1 spec |
| `moats/phase-1/M2-anticipatory-preloading.md` | M2 spec |
| `moats/phase-1/M3-causal-execution-traces.md` | M3 spec |
| `moats/phase-2/M4-M5-M6-M9-*.md` | Phase 2 specs |
| `moats/phase-3/M7-M8-*.md` | Phase 3 specs |
| `moats/phase-4/M10-*.md` | Phase 4 spec |
| `../memory_implementation_plan.md` | Memory foundation |

## Structural Advantage

```
tamux daemon owns:
 ├── Terminal (PTY)
 ├── Task queue
 ├── Goal runners
 ├── Agent loop
 └── Memory system
     └── ALL IN ONE PROCESS
     
Competitors have: separate agent, memory, task systems
→ They must INTEGRATE across boundaries
→ tamux WIRES moats NATIVE inside the system
```
