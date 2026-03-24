# Phase 3 Moats — Spec

**Phase:** 3  
**Status:** Draft  
**Prerequisites:** Phase 1 + Phase 2

---

## Overview

Phase 3 moats are advanced features for multi-agent collaboration and regulated environments:
- **M7**: Multi-Agent Collaboration Protocol
- **M8**: Trusted Execution Provenance

These are higher risk and effort — only pursue after Phase 2 is stable.

---

## M7: Multi-Agent Collaboration Protocol

### Vision

Goal runners do fork-join: one agent orchestrates sub-agents. Real collaboration needs **peer reasoning**:
- Agents read each other's working memory
- Agents disagree and surface structured conflicts
- Agents vote with confidence weights
- Agents escalate to operator only when necessary

### Collaboration Model

```
┌─────────────────────────────────────────────────────┐
│  Mission: "Refactor auth module for new API"        │
└─────────────────────────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
    ┌─────────┐    ┌─────────┐    ┌─────────┐
    │ Agent A │    │ Agent B │    │ Agent C │
    │Research │    │Execution│    │ Review  │
    └────┬────┘    └────┬────┘    └────┬────┘
         │              │              │
         ▼              ▼              ▼
    ├─ Found 3          ├─ Applies      ├─ Audits
    │  approaches       │  approach 2   │  approach 2
    │                   │               │
    │                   ▼               │
    │              ⚠️ Conflict:         │
    │              "Approach 2 uses     │
    │               deprecated API"     │
    │                   │               │
    │                   ▼               ▼
    └───────→ Peer disagreement surfaced
                      to operator
```

### Data Model

```rust
struct CollaborationSession {
    id: Uuid,
    mission: String,
    parent_goal_run: Uuid,
    
    agents: Vec<CollaborativeAgent>,
    shared_context: SharedContext,
    disagreements: Vec<Disagreement>,
    consensus: Option<Consensus>,
}

struct CollaborativeAgent {
    id: Uuid,
    role: AgentRole,           // Research, Execution, Review, etc.
    working_memory: WorkingMemory,
    status: AgentStatus,
    contributions: Vec<Contribution>,
}

enum AgentRole {
    Research,    // Finds options, analyzes constraints
    Execution,   // Implements, runs commands
    Review,      // Audits, checks quality
    Planning,    // Coordinates, resolves conflicts
}

struct WorkingMemory {
    agent_id: Uuid,
    current_hypothesis: String,
    evidence: Vec<Evidence>,
    confidence: f64,
    pending_questions: Vec<String>,
}

struct SharedContext {
    facts: Vec<SharedFact>,      // Agreed-upon facts
    hypotheses: Vec<Hypothesis>, // Under discussion
    constraints: Vec<Constraint>,
}

struct Disagreement {
    id: Uuid,
    agents: Vec<AgentId>,         // Agents in disagreement
    topic: String,
    position_a: AgentPosition,
    position_b: AgentPosition,
    evidence_a: Vec<Evidence>,
    evidence_b: Vec<Evidence>,
    resolution: DisagreementResolution,
}

enum DisagreementResolution {
    Resolved { winner: AgentId, rationale: String },
    Escalated { escalated_to: EscalationTarget },
    Pending,
}
```

### Agent-to-Agent Protocol

#### 7.1 Shared Context Read

Agents can read each other's working memory:

```rust
async fn read_peer_memory(peer_id: AgentId) -> WorkingMemory {
    collaboration_session.get_agent(peer_id).working_memory.clone()
}
```

#### 7.2 Contribution Broadcast

When an agent makes a contribution, it broadcasts to peers:

```rust
async fn broadcast_contribution(agent_id: AgentId, contribution: Contribution) {
    for peer in collaboration_session.agents.iter().filter(|a| a.id != agent_id) {
        peer.receive_contribution(contribution.clone()).await;
    }
}
```

#### 7.3 Conflict Detection

```rust
fn detect_conflict(shared: &SharedContext) -> Option<Disagreement> {
    // Check if two agents have contradictory positions
    for (i, hypo_a) in shared.hypotheses.iter().enumerate() {
        for hypo_b in shared.hypotheses.iter().skip(i + 1) {
            if semantically_contradicts(hypo_a, hypo_b) {
                return Some(Disagreement {
                    agents: vec![hypo_a.source_agent, hypo_b.source_agent],
                    topic: find_common_topic(hypo_a, hypo_b),
                    position_a: hypo_a.position.clone(),
                    position_b: hypo_b.position.clone(),
                    // ...
                });
            }
        }
    }
    None
}
```

#### 7.4 Voting Protocol

For recoverable disagreements:

```rust
async fn vote_on(disagreement: &mut Disagreement, session: &CollaborationSession) -> AgentId {
    let mut votes: HashMap<AgentId, u32> = HashMap::new();
    
    for agent in &session.agents {
        let vote = agent.delegate_vote(disagreement).await;
        // Weight by confidence and expertise
        let weighted_vote = vote * agent.expertise_weight(disagreement.topic);
        *votes.entry(vote).or_insert(0) += weighted_vote;
    }
    
    votes.into_iter().max_by_key(|(_, v)| *v).map(|(a, _)| a).unwrap()
}
```

### UI Surface

```
┌─────────────────────────────────────────────────────────────────┐
│ Collaboration: "Refactor auth module"                            │
├─────────────────────────────────────────────────────────────────┤
│ Agents                                                          │
│ ├── 🔬 Research (Agent A) — analyzing options                  │
│ ├── ⚙️ Execution (Agent B) — implementing approach 2           │
│ └── 📋 Review (Agent C) — auditing                              │
├─────────────────────────────────────────────────────────────────┤
│ Shared Context                                                  │
│ ✓ Uses OAuth2 flow (confirmed)                                  │
│ ✓ Must support refresh tokens (confirmed)                      │
│ ? Approach 2 vs 3 (under discussion)                           │
├─────────────────────────────────────────────────────────────────┤
│ ⚠️ Disagreement                                                 │
│                                                                 │
│ Topic: "Which auth library to use?"                            │
│                                                                 │
│ Agent B (Execution):                                           │
│   "Use oauth2 crate — familiar, fast to implement"             │
│   Evidence: 3 prior projects used it successfully              │
│                                                                 │
│ Agent C (Review):                                               │
│   "Use auth-middleware — better error handling"               │
│   Evidence: oauth2 crate has 2 known edge case bugs            │
│                                                                 │
│ Agent A (Research):                                            │
│   "Supporting both: auth-middleware with oauth2 fallback"      │
│                                                                 │
│                          [Vote] [Escalate to Operator]        │
└─────────────────────────────────────────────────────────────────┘
```

### Wire Into Existing Modules

- `agent/dispatcher.rs` → multi-agent task spawning
- `agent/messaging.rs` → agent-to-agent message protocol
- `agent/metacognitive/escalation.rs` → operator arbitration trigger

---

## M8: Trusted Execution Provenance

### Vision

For regulated or high-stakes environments, every decision can be cryptographically attested:
- Sign + timestamp every goal, plan, step, approval
- Provable chain from goal → plan → execution → outcome
- Tamper-evident append-only log
- SOC2-compatible compliance mode

### Data Model

```rust
struct ProvenanceEntry {
    sequence: u64,              // Monotonic counter
    timestamp: DateTime<Utc>,
    
    // What happened
    event_type: ProvenanceEvent,
    
    // Chain linkage
    prev_hash: Sha256,          // Hash of previous entry
    entry_hash: Sha256,         // Hash of this entry
    
    // Attestation
    agent_id: Uuid,
    goal_run_id: Option<Uuid>,
    signature: Option<Ed25519Signature>,  // If signing enabled
    
    // Payload
    payload: ProvenancePayload,
}

enum ProvenanceEvent {
    GoalCreated,
    PlanGenerated,
    StepStarted,
    StepCompleted,
    StepFailed,
    ToolCall,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalDenied,
    EscalationTriggered,
    ReplanTriggered,
    RecoveryTriggered,
    ContextCompressed,
}

struct ProvenancePayload {
    summary: String,
    details: serde_json::Value,
    causal_trace_id: Option<Uuid>,  // Link to M3 causal trace
}
```

### Hash Chain

```rust
impl ProvenanceEntry {
    fn compute_hash(&self) -> Sha256 {
        let mut hasher = Sha256::new();
        hasher.update(self.sequence.to_le_bytes());
        hasher.update(self.timestamp.to_rfc3339());
        hasher.update(serde_json::to_string(&self.event_type).unwrap());
        hasher.update(self.prev_hash);
        hasher.update(serde_json::to_string(&self.payload).unwrap());
        Sha256::digest(hasher)
    }
    
    fn sign(&mut self, key: &SigningKey) {
        self.signature = Some(key.sign(self.entry_hash.as_bytes()));
    }
    
    fn verify(&self) -> bool {
        // Verify hash chain integrity
        if self.entry_hash != self.compute_hash() {
            return false;
        }
        // Verify signature
        if let Some(sig) = &self.signature {
            return self.agent_key.verify(self.entry_hash.as_bytes(), sig);
        }
        true
    }
}
```

### Tamper Detection

```rust
struct ProvenanceLog {
    entries: Vec<ProvenanceEntry>,
    signing_enabled: bool,
}

impl ProvenanceLog {
    fn verify_chain(&self) -> Result<(), ChainVerificationError> {
        for (i, entry) in self.entries.iter().enumerate() {
            if i == 0 {
                // Genesis block — no prev_hash
                continue;
            }
            
            // Verify hash chain
            let prev = &self.entries[i - 1];
            if entry.prev_hash != prev.entry_hash {
                return Err(ChainVerificationError::BrokenLink {
                    entry: i,
                    expected: prev.entry_hash,
                    found: entry.prev_hash,
                });
            }
            
            // Verify entry integrity
            if !entry.verify() {
                return Err(ChainVerificationError::TamperedEntry {
                    entry: i,
                    reason: "hash or signature mismatch",
                });
            }
        }
        Ok(())
    }
}
```

### Compliance Mode

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplianceConfig {
    pub mode: ComplianceMode,
    pub retention_days: u32,
    pub sign_all_events: bool,
    pub audit_log_path: PathBuf,
}

pub enum ComplianceMode {
    Standard,     // Normal operation, no special handling
    SOC2,         // SOC2 Type II compliance mode
    HIPAA,        // Healthcare compliance
    FedRAMP,      // US Government compliance
}

impl ComplianceConfig {
    pub fn for_mode(mode: ComplianceMode) -> Self {
        match mode {
            ComplianceMode::SOC2 => ComplianceConfig {
                mode,
                retention_days: 90,
                sign_all_events: true,
                audit_log_path: PathBuf::from("~/.tamux/audit/soc2/"),
            },
            // ... other modes
        }
    }
}
```

### SOC2 Artifact Generation

```rust
struct SOC2Artifacts {
    audit_log: ProvenanceLog,
    evidence_report: EvidenceReport,
    access_log: AccessLog,
}

struct EvidenceReport {
    generated_at: DateTime<Utc>,
    period: DateRange,
    
    // Required sections for SOC2
    change_management: Vec<ChangeRecord>,
    system_access: Vec<AccessRecord>,
    data_integrity: Vec<IntegrityCheck>,
    incident_log: Vec<IncidentRecord>,
}
```

### Wire Into Existing Modules

- `agent/learning/traces.rs` → sign causal trace events
- `agent/metacognitive/escalation.rs` → audit on escalation
- `agent/task_crud.rs` → sign goal/step events
- `agent/dispatcher.rs` → sign approval events

---

## Dependencies

```
Phase 1 + 2
    │
    ├── M3 (Causal Traces) ──→ M8 (sign causal events)
    │
    ├── M1 (Operator Model) ──→ M7 (agent roles, preferences)
    │
    └── M4, M5 ──→ M7 (shared context from skills/env model)
```

---

## Milestones

### M7: Multi-Agent Protocol
- [ ] M7.1: Shared context space for sub-agents
- [ ] M7.2: Agent-to-agent message protocol
- [ ] M7.3: Conflict detection on positions
- [ ] M7.4: Voting protocol with confidence weights
- [ ] M7.5: Structured disagreement surface (UI)
- [ ] M7.6: Operator escalation flow

### M8: Trusted Provenance
- [ ] M8.1: Provenance entry data model
- [ ] M8.2: Hash chain implementation
- [ ] M8.3: Sign all goal/plan/step events
- [ ] M8.4: Tamper detection on log corruption
- [ ] M8.5: Compliance mode configuration
- [ ] M8.6: SOC2 artifact generation

---

## Risk Assessment

| Moat | Risk | Mitigation |
|------|------|------------|
| M7 | 🔴 High — complex coordination | Start with 2-agent, expand gradually |
| M8 | 🟡 Medium — key management | Use local keys, no external KMS dependency |

---

## Test Plan

### M7: Multi-Agent
1. Spawn 2 agents with shared context, verify they can read each other's memory
2. Simulate disagreement, verify structured surface
3. Test voting protocol with known outcomes

### M8: Trusted Provenance
1. Run 10 goal sessions with signing enabled, verify chain integrity
2. Tamper with log, verify detection
3. Generate SOC2 report, verify required sections
