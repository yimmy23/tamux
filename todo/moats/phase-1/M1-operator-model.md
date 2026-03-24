# M1: Operator Model — Spec

**Moat:** "The agent that knows how you think"  
**Phase:** 1  
**Status:** Draft  

---

## Problem Statement

tamux currently stores operator preferences as static files (`USER.md`, ~139 chars, bare-bones). The agent has no model of *how* the operator thinks — their cognitive style, risk tolerance, attention patterns, or workflow rhythms.

Every other agent framework treats the operator as a static context window. The Operator Model treats the human as a dynamic system to be understood.

---

## Vision

The agent builds and maintains a rich, privacy-preserving model of the operator that enables:

- **Adaptive output density**: Terse for fast typers, verbose for careful readers
- **Calibrated approvals**: "I know you'll want to see this" — pre-screen based on learned risk fingerprint
- **Proactive timing**: Pre-warm context when the operator typically starts sessions
- **Panel placement**: Surface information where the operator actually looks
- **Implicit corrections**: Learn from what the operator ignores vs. engages with

---

## Components

### 1.1 Cognitive Style Profiler

**What it tracks:**
- Message length distribution (avg words per operator message)
- Question frequency (does the operator ask clarifying questions?)
- Confirmation seeking (do they ask "are you sure?" or just approve?)
- Error response patterns (do they read error traces fully? skip to solutions?)

**Data model:**
```rust
struct CognitiveStyle {
    avg_message_length: f64,        // words per message
    question_frequency: f64,       // questions / total messages
    confirmation_seeking: f64,      // confirmations asked / tool calls
    error_response_depth: f64,     // 0.0-1.0 (skims vs. reads fully)
    
    // Behavioral signals
    session_start_hour: Option<u8>, // 24h, if consistent
    session_duration_avg: f64,      // minutes
    break_frequency: Option<f64>,   // breaks per hour, if detectable
    
    // Computed
    verbosity_preference: Verbosity, // Terse | Moderate | Verbose
    reading_depth: ReadingDepth,     // Skim | Standard | Deep
}
```

**Collection points:**
- Every operator message → update running statistics
- Session metadata → track timing patterns
- Approval/denial events → track confirmation seeking

### 1.2 Risk Tolerance Fingerprint

**What it tracks:**
- Which approval categories the operator grants vs. denies
- Pattern of auto-denied command classes (if any)
- Time-to-approve: how quickly do they respond to approvals?
- Escalation preference: do they prefer to be asked or trust the agent?

**Data model:**
```rust
struct RiskFingerprint {
    // Approval patterns by category
    approval_rate_by_category: HashMap<CommandCategory, f64>,
    // e.g., {"destructive_delete": 0.1, "network_request": 0.8, "file_write": 0.9}
    
    avg_response_time_secs: f64,
    
    // Computed
    risk_tolerance: RiskTolerance, // Conservative | Moderate | Aggressive
    
    // Learned shortcuts
    skip_approvals_for: Vec<CommandPattern>, // "always allow git commits"
    always_approve_patterns: Vec<CommandPattern>,
}
```

**Wire into:**
- Approval pre-screening: "This command matches your approved pattern X, proceeding without pause"
- Output surfacing: More blast radius detail for conservative operators

### 1.3 Session Rhythm Model

**What it tracks:**
- Typical session start time
- Session duration and end patterns
- Active vs. idle periods within sessions
- Deep work blocks vs. quick check-ins

**Data model:**
```rust
struct SessionRhythm {
    // Learned from session history
    typical_start_hour: Option<u8>,
    typical_start_minute: Option<u8>,
    typical_end_hour: Option<u8>,
    
    session_duration_p50: f64,  // median minutes
    session_duration_p95: f64,
    
    // Activity patterns
    peak_activity_hours: Vec<u8>,  // 24h clock
    
    // Computed
    is_morning_session: bool,
    is_deep_work_session: bool,  // > 60 min, low idle rate
}
```

**Wire into:**
- M2: Morning brief scheduling
- M2: Pre-warm context at session start
- M2: Idle detection (operator away)

### 1.4 Attention Topology

**What it tracks:**
- Which panels does the operator actually view during sessions?
- How long do they spend in Mission Control vs. Terminal vs. Chat?
- What do they ignore? (never opens certain panels)

**Data model:**
```rust
struct AttentionTopology {
    panel_focus_time: HashMap<PanelId, Duration>,
    panel_visit_count: HashMap<PanelId, u32>,
    
    // Computed
    primary_focus_panel: PanelId,    // where they spend most time
    secondary_panels: Vec<PanelId>,
    ignored_panels: Vec<PanelId>,     // visited < 2 times in 30 sessions
}
```

**Wire into:**
- M2: Surface information in the right panel
- Output format: prefer terminal for primary-terminal operators, chat for others

---

## Storage

Operator model is stored in:
```
~/.tamux/agent-mission/operator_model.json
```

Structure:
```json
{
  "version": "1.0",
  "last_updated": "2024-01-15T10:30:00Z",
  "session_count": 47,
  "cognitive_style": { ... },
  "risk_fingerprint": { ... },
  "session_rhythm": { ... },
  "attention_topology": { ... }
}
```

**Privacy considerations:**
- No raw messages stored, only aggregate statistics
- All computation happens locally
- Model is operator-owned, never transmitted
- Operator can export or purge model at any time

---

## Wire Into Agent Loop

### Injection into system prompt (M1.1, M1.2):
```
OPERATOR MODEL (learned, may be overridden)
- Output preference: MODERATE (you average 40 words per message)
- Risk tolerance: MODERATE (you approve 70% of destructive commands)
- Approval shortcut: "git commit" always approved
- You prefer terminal output over chat summaries
```

### Approval pre-screening (M1.2):
```rust
// Before requesting approval, check if operator has auto-approved this pattern
if operator_model.risk_fingerprint.skip_approvals_for.contains(&pattern) {
    return ApprovalResult::AutoApproved;
}

// Adjust blast radius detail based on risk tolerance
let blast_radius_detail = match operator_model.risk_fingerprint.risk_tolerance {
    Conservative => DetailLevel::Full,
    Moderate => DetailLevel::Standard,
    Aggressive => DetailLevel::Minimal,
};
```

### Session start (M1.3):
```rust
// On session start, check if this matches a typical start time
if operator_model.session_rhythm.is_within_start_window(now) {
    trigger_morning_brief();  // M2 behavior
    prewarm_context();        // M2 behavior
}
```

---

## Milestones

- [ ] M1.1: Cognitive style profiler — message length, question freq, confirmation seeking
- [ ] M1.2: Risk tolerance fingerprint — approval patterns by category
- [ ] M1.3: Session rhythm model — start/end patterns, deep work detection
- [ ] M1.4: Attention topology — panel focus tracking
- [ ] M1.5: System prompt injection — adaptive output based on model
- [ ] M1.6: Approval pre-screening — learned shortcuts
- [ ] M1.7: Session start hooks — rhythm-aware pre-warming trigger

---

## Dependencies

- **Enables:** M2 (pre-warm timing), M9 (behavioral learning loop)
- **Uses:** Session metadata, approval events, message statistics
- **Conflicts:** None

---

## Test Plan

1. **Simulate sessions**: Generate synthetic operator behavior, verify model converges
2. **Manual testing**: Use tamux for 10 sessions, verify model captures patterns
3. **Override test**: Explicitly contradict learned model, verify agent adapts
4. **Privacy test**: Verify no raw messages in model file, only aggregates
