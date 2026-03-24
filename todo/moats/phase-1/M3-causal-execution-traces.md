# M3: Causal Execution Traces — Spec

**Moat:** "Why did you do that?"  
**Phase:** 1  
**Status:** Draft  

---

## Problem Statement

tamux has `traces.rs` (execution traces) and `patterns.rs` (pattern mining), but these capture *what* happened — not *why* the agent made the decisions it did.

When a tool fails, the agent knows it failed. But does it know *why* it chose that tool over alternatives? When a plan succeeds, does it know which context factors contributed?

Causal traces turn a log into a knowledge base.

---

## Vision

Every agent decision is attributed with:
1. **What options were considered** (not just what was picked)
2. **Why each was rejected** (reasoning at decision time)
3. **What context factors influenced** the choice
4. **What the counterfactual would have been** (what if it had picked differently?)

This enables:
- **Counterfactual reasoning**: "In sessions with similar context, tool A failed; tool B succeeded"
- **Blast radius prediction**: "This operation resembles one that caused X failure last time"
- **Learning from near-misses**: What almost went wrong but was caught?
- **Justifiable decisions**: The agent can explain *why* it did something

---

## Data Model

### Core: CausalTrace

```rust
struct CausalTrace {
    trace_id: Uuid,
    session_id: Uuid,
    goal_run_id: Option<Uuid>,
    
    // Decision metadata
    decision_type: DecisionType,  // ToolSelection | PlanChoice | Replan | Escalate
    timestamp: DateTime<Utc>,
    
    // The decision
    selected: DecisionOption,
    rejected_options: Vec<DecisionOption>,
    
    // Context at decision time (what influenced the choice)
    context_hash: Sha256,        // Hash of context that was in scope
    causal_factors: Vec<CausalFactor>,
    
    // Outcome
    outcome: TraceOutcome,
    outcome_reason: Option<String>,
    
    // Attribution
    model_used: String,
    reasoning_token_count: u32,
}

enum DecisionType {
    ToolSelection,
    PlanChoice,
    Replan,
    Escalate,
    ContextCompression,
    SkillSelection,
}

struct DecisionOption {
    option_id: Uuid,
    option_type: String,           // "read_file", "grep", "plan_A", etc.
    reasoning: String,             // Why the agent considered this
    rejection_reason: Option<String>, // If rejected, why
    estimated_success_prob: f64,   // Agent's internal estimate
}

struct CausalFactor {
    factor_type: FactorType,
    description: String,
    weight: f64,  // How much this factor influenced the decision
}

enum FactorType {
    PastSuccess,        // "Tool X succeeded last time in this context"
    PastFailure,       // "Tool Y failed last time with similar files"
    ContextPresence,   // "File Z was in context"
    OperatorPreference, // M1: "Operator prefers terminal output"
    ResourceConstraint, // "Context was low, chose cheaper option"
    PatternMatch,      // "Matched heuristic: files > 1000 lines use grep"
}

enum TraceOutcome {
    Success,
    PartialSuccess,
    Failure,
    NearMiss { what_went_wrong: String, how_recovered: String },
    Unresolved,
}
```

### Wire format (for LLM injection):

```
DECISION RECORD: tool_selection
Timestamp: 2024-01-15T14:23:01Z

Chose: read_file(path="src/main.rs")
Rejected:
  - grep(pattern="fn main") → "overkill for single file"
  - bash_command("head -100") → "no benefit over read_file"

Causal factors:
  - past_success: read_file succeeded 12 times this session
  - context_presence: file was already in context from cargo tree output
  - resource_constraint: context was 70% full

Outcome: success
```

---

## Capture Points

### 3.1 Tool Selection Traces

**Where:** In `agent/agent_loop.rs`, when the LLM returns a tool call.

**What to capture:**
```rust
// Current: just log the tool call
log_tool_call(tool_name, args);

// New: capture the decision
CausalTrace {
    decision_type: ToolSelection,
    selected: DecisionOption { option_type: tool_name, ... },
    rejected_options: inferred_rejections,  // From LLM reasoning if available
    causal_factors: compute_causal_factors(context, history),
    ...
}
```

**Challenge:** The LLM doesn't always output why it rejected other tools. 
**Solution:** 
1. Add a `reasoning_for_choice` field to tool call requests
2. Or infer from context: if `grep` was considered, why didn't the agent pick it?
3. Use post-hoc reasoning from reflection step

### 3.2 Plan Choice Traces

**Where:** In `agent/goal_planner.rs`, when selecting between plan variants.

**What to capture:**
```rust
// When LLM proposes multiple plans
let plan_traces: Vec<CausalTrace> = plans.iter().map(|p| {
    CausalTrace {
        decision_type: PlanChoice,
        selected: p,
        rejected_options: other_plans,
        causal_factors: [
            CausalFactor { 
                factor_type: FactorType::PastSuccess,
                description: "Plan A structure succeeded in similar refactor goals",
                weight: 0.4,
            },
            // ...
        ],
        ...
    }
}).collect();
```

### 3.3 Replan Traces

**Where:** In `agent/metacognitive/replanning.rs`.

**What to capture:**
```rust
CausalTrace {
    decision_type: Replan,
    context: current_plan_state,
    causal_factors: [
        CausalFactor {
            factor_type: FactorType::PastFailure,
            description: "Step 3 failed with identical error 3 times",
            weight: 0.9,
        },
    ],
    outcome: replan_result.is_better ? Success : Failure,
}
```

### 3.4 Near-Miss Traces

**Where:** In `agent/liveness/recovery.rs`, when recovery catches an error.

```rust
CausalTrace {
    decision_type: ToolSelection,
    outcome: NearMiss {
        what_went_wrong: "read_file hit permission error on first try",
        how_recovered: "auto-retried with sudo",
    },
}
```

---

## Usage Patterns

### 3.5 Blast Radius Prediction

Before executing a command, query causal history:

```rust
fn predict_blast_radius(command: &str) -> BlastRadiusPrediction {
    let similar_traces = causal_store.search_by_command_type(command);
    
    // Find traces with similar operations that failed
    let failures = similar_traces.iter()
        .filter(|t| matches!(t.outcome, Failure | NearMiss { .. }));
    
    if failures.is_empty() {
        return BlastRadiusPrediction { risk: Low, evidence: None };
    }
    
    // Aggregate failure patterns
    let common_factors: Vec<_> = failures
        .flat_map(|f| &f.causal_factors)
        .collect();
    
    let risk = if failures.len() > 3 { High } else { Medium };
    let evidence = format!(
        "Similar operation failed {} times. Common cause: {}",
        failures.len(),
        summarize_factors(&common_factors)
    );
    
    BlastRadiusPrediction { risk, evidence: Some(evidence) }
}
```

### 3.6 Counterfactual Reasoning

```rust
fn get_counterfactual(tool: &str, context: &Context) -> Option<Counterfactual> {
    let rejected_trace = causal_store.find_similar_rejection(tool, context)?;
    
    // What would have happened if we picked the rejected option?
    Some(Counterfactual {
        rejected_option: rejected_trace.option_type.clone(),
        rejection_reason: rejected_trace.rejection_reason.clone(),
        // We can't know for sure, but we can infer from history
        estimated_outcome: estimate_from_history(&rejected_trace, context),
    })
}
```

### 3.7 Tool Selection Guidance

```rust
fn suggest_tool(context: &Context) -> ToolSuggestion {
    let causal_history = causal_store.get_recent_traces(context.session_id);
    
    // Find tools that succeeded in similar context
    let successes = causal_history.iter()
        .filter(|t| matches!(t.outcome, Success))
        .filter(|t| context_hash_similar(&t.context_hash, &context.hash()))
        .collect::<Vec<_>>();
    
    let recommended = aggregate_successful_tools(&successes);
    let warned = find_cautionary_patterns(&causal_history, context);
    
    ToolSuggestion { recommended, warned }
}
```

---

## Storage

### SQLite Schema

```sql
CREATE TABLE causal_traces (
    trace_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    goal_run_id TEXT,
    decision_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    selected_json TEXT NOT NULL,        -- DecisionOption JSON
    rejected_json TEXT NOT NULL,       -- Vec<DecisionOption> JSON
    context_hash TEXT NOT NULL,
    causal_factors_json TEXT NOT NULL, -- Vec<CausalFactor> JSON
    outcome_json TEXT NOT NULL,        -- TraceOutcome JSON
    model_used TEXT,
    reasoning_token_count INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_causal_session ON causal_traces(session_id);
CREATE INDEX idx_causal_decision_type ON causal_traces(decision_type);
CREATE INDEX idx_causal_outcome ON causal_traces(outcome_json);
CREATE INDEX idx_causal_context ON causal_traces(context_hash);
```

---

## Wire Into Existing Modules

### `agent/learning/traces.rs`
Extend with causal fields, add `causal_store` module.

### `agent/learning/patterns.rs`
Mine causal chains, not just co-occurrence.

### `agent/metacognitive/`
Use causal history for tool selection and replanning.

### `agent/context/`
Wire blast radius prediction into approval pre-screening.

---

## Milestones

- [ ] M3.1: Data model — CausalTrace, DecisionOption, CausalFactor structs
- [ ] M3.2: Tool selection traces — capture decision context in agent_loop
- [ ] M3.3: Plan choice traces — capture in goal_planner
- [ ] M3.4: Replan traces — capture in metacognitive/replanning
- [ ] M3.5: Near-miss tracking — capture in recovery
- [ ] M3.6: SQLite storage — causal_traces table, queries
- [ ] M3.7: Blast radius prediction — pre-execution warning surface
- [ ] M3.8: Counterfactual surface — "what if I had picked X?" query
- [ ] M3.9: Tool selection guidance — LLM prompt injection with history

---

## Dependencies

- **Depends on:** None (can be built standalone)
- **Enables:** M4 (causal skill variants), M5 (environment-aware decisions), M8 (signing)
- **Conflicts:** None

---

## Test Plan

1. **Trace completeness**: Run 20 goal sessions, verify every tool call has a causal trace
2. **Factor accuracy**: Spot-check causal factors — are they accurate?
3. **Blast radius test**: Run operation that failed before, verify prediction surfaces
4. **Counterfactual test**: Verify counterfactual query returns relevant history
5. **Storage test**: Query performance with 10k traces
