# Phase 2 Moats — Spec

**Phase:** 2  
**Status:** Draft  
**Prerequisites:** Phase 1 (M1, M2, M3)

---

## Overview

Phase 2 moats build on Phase 1 foundations to create an intelligence layer that:
- Evolves skills automatically based on usage context (M4)
- Understands the operator's environment semantically (M5)
- Stores knowledge with provenance and consistency checking (M6)
- Learns from implicit behavioral signals (M9)

---

## M4: Genetic Skill Evolution

### Vision

Skills are currently static documents. Make them evolve like a codebase:
- **Branching**: When a skill is used in a new context, create a variant
- **Competition**: Track which variant succeeds in which context
- **Pruning**: Archive variants that fall below success threshold
- **Merging**: Convergent variants merge back to canonical

### Branching Model

```
skill: debug-rust-stack-overflow
├── canonical (v3.0) — base approach, always tried first
├── legacy-code (v3.1) — aggressive stack limits for old crates
├── wasm-target (v3.2) — wasm32-unknown-unknown specific
└── async-runtime (v3.3) — tokio stack hints for async code
```

### Data Model

```rust
struct SkillVariant {
    variant_id: Uuid,
    skill_name: String,
    parent_id: Option<Uuid>,     // None for canonical
    version: SemanticVersion,
    
    // Content
    content: String,
    when_to_use: String,
    
    // Usage tracking
    context_tags: Vec<ContextTag>,  // ["legacy-crate", "wasm32", "async"]
    use_count: u32,
    success_count: u32,
    failure_count: u32,
    
    // Computed
    success_rate: f64,
    last_used: DateTime<Utc>,
    status: VariantStatus,
}

enum ContextTag {
    Language(String),         // "rust", "python"
    Platform(String),        // "wasm32", "linux"
    Domain(String),           // "async", "database", "frontend"
    Scale(String),           // "monolith", "microservice"
    Custom(String),           // anything else
}

enum VariantStatus {
    Active,
    Deprecated,
    Archived,
    PromotedToCanonical,
}
```

### Variant Selection

```rust
fn select_variant(skill: &Skill, context: &ExecutionContext) -> &SkillVariant {
    // 1. Check for exact context match
    if let Some(v) = skill.variants.iter().find(|v| 
        v.context_tags.matches(&context.tags) && v.status == Active
    ) {
        return v;
    }
    
    // 2. Fuzzy match — partial tag overlap, sorted by success rate
    let candidates: Vec<_> = skill.variants.iter()
        .filter(|v| v.status == Active)
        .filter(|v| v.context_tags.overlaps(&context.tags))
        .collect();
    
    if !candidates.is_empty() {
        // Return highest success rate with any overlap
        return candidates.iter().max_by_key(|v| v.success_rate).unwrap();
    }
    
    // 3. Fall back to canonical
    skill.canonical()
}
```

### Evolution Rules

```rust
struct EvolutionRules {
    // Create new variant when:
    min_context_mismatch_to_branch: 2,   // 2+ new context tags
    min_uses_to_branch: 3,               // used 3+ times with new context
    
    // Archive when:
    success_rate_threshold: 0.3,         // below 30% success
    max_age_without_use: Duration::days(90),
    
    // Merge when:
    similarity_threshold: 0.8,           // 80% content overlap
    both_stable: bool,                   // both Active for 30+ days
}
```

---

## M5: Semantic Environment Model

### Vision

Not just files in a repo — a rich, queryable model of the operator's world:
- **Dependency graph**: Crates, packages, services and their relationships
- **Infrastructure ontology**: Docker, K8s, Terraform topology
- **Convention library**: Hidden patterns from code analysis and operator corrections
- **Temporal context**: "Last 3 deployments were Tuesdays and caused incidents"

### Model Components

#### 5.1 Dependency Graph

```rust
struct DependencyGraph {
    nodes: HashMap<NodeId, DependencyNode>,
    edges: Vec<DependencyEdge>,
}

struct DependencyNode {
    id: NodeId,
    node_type: NodeType,
    name: String,
    version: Option<String>,
    metadata: HashMap<String, String>,
}

enum NodeType {
    Crate, Package, Service, Container, Function, Module,
}

struct DependencyEdge {
    from: NodeId,
    to: NodeId,
    edge_type: EdgeType,
}

enum EdgeType {
    DependsOn,
    Imports,
    Calls,
    DeploysTo,
}
```

**Extraction sources:**
- `Cargo.toml` → crate graph
- `package.json` → npm graph
- `docker-compose.yml` → service graph
- `terraform state` → infra graph
- Import statements → code graph

#### 5.2 Convention Library

```rust
struct ConventionLibrary {
    conventions: Vec<Convention>,
}

struct Convention {
    id: Uuid,
    pattern: String,            // regex or code pattern
    description: String,
    source: ConventionSource,
    confidence: f64,
    file_pattern: Option<String>, // "*.rs", "src/**/*.ts"
}

enum ConventionSource {
    CodeAnalysis,
    Transcript,                 // derived from past sessions
    OperatorCorrection,        // explicit override
    CIAnalysis,                // from CI logs
}
```

**Examples:**
- "PRs must include CHANGELOG entry" (derived from CI analysis)
- "Error types go in `src/error.rs`" (derived from code analysis)
- "Never modify `legacy/` without running tests" (derived from transcript)

#### 5.3 Semantic Query Surface

```rust
fn semantic_query(query: &str, context: &ExecutionContext) -> QueryResult {
    match query_type(query) {
        QueryType::Dependency => {
            // "What depends on the database module?"
            resolve_dependency_chain(context.target)
        }
        QueryType::Convention => {
            // "What's the convention for error handling here?"
            lookup_convention(context.file_path)
        }
        QueryType::Temporal => {
            // "Has this operation caused issues before?"
            temporal_failure_history(context.operation)
        }
    }
}
```

---

## M6: Deep Storage Architecture

### Vision

A hybrid vector + knowledge graph store for long-term memory that:
- Tracks provenance: "I know this fact because..."
- Has temporal decay with preservation: unused facts fade but aren't deleted
- Detects contradictions before silent overwrite

### Data Model

```rust
struct MemoryFact {
    id: Uuid,
    content: String,
    
    // Provenance
    source_session: SessionId,
    source_type: FactSource,
    derived_from: Vec<FactId>,   // if derived, what source facts
    confirmed_by: Vec<SessionId>, // operator confirmed this fact
    
    // Temporal
    created_at: DateTime<Utc>,
    last_used: DateTime<Utc>,
    confidence: f64,              // 0.0-1.0, decays over time
    
    // Relationships
    contradictions: Vec<FactId>,
    supports: Vec<FactId>,
    tags: Vec<String>,
}

enum FactSource {
    OperatorStated,    // operator explicitly stated
    Derived,           // inferred from context
    ToolOutput,       // extracted from command output
    Transcript,       // parsed from session transcript
    Skill,            // extracted from skill document
}
```

### Provenance Tracking

Every fact in MEMORY.md gets a provenance header:

```markdown
# MEMORY.md

## Facts

### Fact: "Project uses tokio 1.x for async"
- **Source**: Derived (2024-01-15, session abc123)
- **Confidence**: 0.95
- **Confirmed by**: operator (2024-01-18, session def456)
- **Last used**: 2024-01-20

### Fact: "Never run tests on Friday"
- **Source**: Transcript (2024-01-10, session ghi789)
- **Confidence**: 0.6 (low — only 1 observation)
- **Tags**: ["testing", "temporal", "deployment"]
- **⚠️ Uncertain**: Not confirmed, may be coincidence

### Fact: "Auth module depends on legacy-oauth crate"
- **Source**: Cargo.toml analysis (2024-01-12)
- **Confidence**: 1.0 (static analysis)
```

### Contradiction Detection

Before updating MEMORY.md:

```rust
fn check_contradiction(new_fact: &MemoryFact, existing_facts: &[MemoryFact]) -> Option<Contradiction> {
    for existing in existing_facts {
        if semantically_contradicts(new_fact, existing) {
            return Some(Contradiction {
                new_fact: new_fact.clone(),
                existing_fact: existing.clone(),
                resolution_needed: true,
            });
        }
    }
    None
}
```

When a contradiction is detected, surface it to the operator instead of silently overwriting:

```
⚠️ Memory Conflict Detected

I want to save: "Use ripgrep instead of grep for large codebases"

But I found a conflicting fact:
  "Use grep for all searches" (from 2024-01-10)

Options:
  [Replace] — overwrite the old fact
  [Keep Both] — save as separate facts
  [Ignore] — don't save the new fact
```

### Temporal Decay

Facts lose confidence over time if not used or confirmed:

```rust
fn decay_confidence(fact: &mut MemoryFact) {
    let days_since_use = Utc::now() - fact.last_used;
    let decay_rate = match fact.source_type {
        OperatorStated => 0.01,  // Very slow decay
        ToolOutput => 0.02,
        Derived => 0.05,
        Transcript => 0.10,       // Fast decay for inferred facts
    };
    
    let decay = decay_rate * days_since_use.num_days() as f64;
    fact.confidence = (fact.confidence - decay).max(0.1); // Never below 0.1
    
    if fact.confidence < 0.3 {
        fact.status = Uncertain;
    }
}
```

**Key rule**: Facts never silently disappear. They enter "uncertain" status and wait for confirmation or purge.

---

## M9: Implicit Feedback Learning

### Vision

Learn from behavior, not just explicit signals. The agent gets smarter just by working with the operator.

### Implicit Signals

#### 9.1 Tool Hesitation

**Signal**: Agent considers tool A, then picks tool B.

```rust
struct ToolHesitation {
    session_id: SessionId,
    timestamp: DateTime<Utc>,
    
    considered: String,           // tool the agent thought about
    selected: String,            // tool the agent picked
    reasoning: String,           // why it picked B over A
    
    // Inferred
    hesitation_reason: HesitationReason,
}

enum HesitationReason {
    EstimatedFailure,    // "A might fail here"
    ResourceConstraint, // "A too expensive given context"
    PatternMismatch,     // "A doesn't match this file type"
    OperatorPreference,  // M1: "Operator prefers B based on history"
}
```

**Use**: Build implicit "don't use X in Y context" heuristics.

#### 9.2 Revision Patterns

**Signal**: Agent tries A, immediately tries B as fallback.

```rust
struct RevisionPattern {
    session_id: SessionId,
    timestamp: DateTime<Utc>,
    
    attempt_1: ToolCall,
    attempt_2: ToolCall,
    time_between_ms: u64,
    
    // Inferred
    pattern_type: RevisionType,
}

enum RevisionType {
    InsufficientOutput,    // A returned too little
    WrongTool,             // A was semantically wrong
    PartialFailure,        // A failed partially, B completed
    OperatorHinted,        // operator typed something that suggested B
}
```

**Use**: "If A fails, try B" is a learned pattern.

#### 9.3 Reading Pattern Inference

**Signal**: Does the operator read full traces or skip to summaries?

```rust
struct ReadingPattern {
    operator_id: OperatorId,
    
    // Tracked over sessions
    avg_trace_length: f64,
    avg_comprehension_time: Duration,
    
    // Computed
    reading_depth: ReadingDepth,
    prefers_summaries: bool,
    skips_reasoning: bool,
}
```

**Wire into M1**:
```rust
if reading_pattern.prefers_summaries {
    // Surface executive summary first, trace on demand
} else {
    // Surface full trace, summary optional
}
```

#### 9.4 Approval Bypass

**Signal**: Operator auto-denies a class of commands without reading details.

```rust
struct ApprovalBypass {
    operator_id: OperatorId,
    
    command_pattern: String,  // regex
    denial_count: u32,
    avg_denial_time_ms: u64,  // fast = didn't read
    
    // Computed
    auto_deny_confidence: f64,  // how certain we are this is automatic
}
```

**Use**: "This matches an auto-deny pattern → skip approval request"

---

## Dependencies for Phase 2

```
M1 (Phase 1) ──┬──→ M2 (Phase 1)
               │
               └──→ M9 ──→ M5
               
M3 (Phase 1) ──→ M4
               ──→ M5
               
M6 (Phase 2) ──→ M4, M5 (knowledge substrate)
```

---

## Milestones

### M4: Genetic Skills
- [ ] M4.1: Skill variant data model
- [ ] M4.2: Context tagging on skill use
- [ ] M4.3: Variant selection with context matching
- [ ] M4.4: Automatic branch creation
- [ ] M4.5: Success rate tracking per variant
- [ ] M4.6: Pruning and merge logic

### M5: Semantic Environment
- [ ] M5.1: Dependency graph extraction
- [ ] M5.2: Convention library from code/transcript
- [ ] M5.3: Temporal context from past sessions
- [ ] M5.4: Semantic query surface for agent

### M6: Deep Storage
- [ ] M6.1: SQLite graph edges extension
- [ ] M6.2: Provenance tracking on MEMORY.md writes
- [ ] M6.3: Confidence decay and uncertain status
- [ ] M6.4: Contradiction detection before writes

### M9: Implicit Feedback
- [ ] M9.1: Tool hesitation tracking
- [ ] M9.2: Revision pattern detection
- [ ] M9.3: Reading pattern inference
- [ ] M9.4: Closed-loop integration with M1

---

## Test Plan

### M4: Genetic Skills
1. Run 10 goal sessions with varied contexts, verify variants created
2. Verify correct variant selected per context
3. Test pruning: create low-success variant, verify archive

### M5: Semantic Environment
1. Parse known Cargo.toml, verify dependency graph
2. Query "what depends on X", verify correct results
3. Derive convention from transcript, verify storage

### M6: Deep Storage
1. Write conflicting fact, verify contradiction surface
2. Let fact decay, verify confidence decreases
3. Verify uncertain facts still present, not deleted

### M9: Implicit Feedback
1. Run session with tool A→B fallback, verify hesitation logged
2. Read trace quickly, verify reading pattern updates
3. Deny same approval class 3 times fast, verify bypass
