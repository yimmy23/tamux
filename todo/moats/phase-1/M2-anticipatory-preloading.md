# M2: Anticipatory Context Pre-loading — Spec

**Moat:** "The agent that acts before you ask"  
**Phase:** 1  
**Status:** Draft  
**Depends on:** M1 (Operator Model)

---

## Problem Statement

Every agent is reactive — it waits for the operator to ask, then responds. This creates friction: context loading delays, tool call waits, and missed opportunities to help.

tamux's structural advantage (daemon owns terminal + agent + memory + task queue) enables proactive behavior that's calibrated to the operator — not annoying, just helpful.

---

## Vision

The agent acts with **contextual anticipation**:
- **Pre-warms** context before the operator starts a session
- **Predicts** what the next step in an active goal will need and loads it
- **Infers** when the operator is stuck and offers targeted help
- **Stays quiet** when confidence is low

The key is calibration: the agent should feel like a thoughtful collaborator, not a pushy assistant.

---

## Behaviors

### 2.1 Morning Brief (Session Start)

**Trigger:** Operator opens tamux at their typical session start time (learned via M1.3).

**What it does:**
```
1. Load MEMORY.md, USER.md, SOUL.md → hydrate context
2. Check for:
   - Unfinished goal runs (from SQLite)
   - Pending approvals
   - New errors in monitored logs
   - Sessions that ended without closure
3. Surface "morning brief" card in Mission Panel:
   
   ┌─────────────────────────────────────┐
   │ Good morning. 3 items for you.      │
   │                                     │
   │ • Goal "Refactor auth" — 2 steps    │
   │   left, paused at approval          │
   │                                     │
   │ • 2 new errors in backend.log      │
   │   (last seen 2h ago)               │
   │                                     │
   │ • Session 2024-01-14 ended without  │
   │   completing the database migration  │
   └─────────────────────────────────────┘
```

**Conditions:**
- Only fires if confidence ≥ 0.8 that this is a "real" session start
- Does not fire for brief reconnects (< 5 min since last session)
- Operator can disable or configure timing window

### 2.2 Predictive Context Hydration

**Trigger:** Active goal run with known next step.

**What it does:**
```
Goal Run "Deploy v2.1" — step 3 of 5
  Step 3: "Run integration tests against staging"
  
  Agent predicts: Will need staging API endpoint, test credentials, 
                 recent deploy logs, integration test suite location
  
  Pre-loads into context (background, before tool call fires):
  - staging-config.json
  - Recent deploy logs (last 50 lines)
  - Test suite manifest
```

**How it works:**
1. Goal planner exposes `predicted_context_needs()` for each step
2. Background task loads predicted files/docs into context archive
3. When step executes, context is already hydrated — no wait time
4. If prediction was wrong, that's logged for M3 (causal trace)

**Confidence threshold:** Only pre-load if ≥ 3 previous steps had predictable context needs.

### 2.3 Implicit Attention Detection

**Trigger:** Operator reads same content for extended period.

**What it does:**
```
Operator has been reading error trace for 45 seconds
  → Agent infers: stuck on this error
  
  Options (based on M1 risk tolerance):
  - High confidence: Surface targeted hint in Mission Panel
  - Medium confidence: Add hint to chat sidebar (non-blocking)
  - Low confidence: Do nothing, wait for explicit question
```

**Detection signals:**
- Cursor position stable on error content
- Repeated scrolls within same region
- No new commands typed for N minutes while viewing error
- Terminal pane has focus

**Confidence calibration:**
- 30s reading + error content → 0.7 confidence
- 30s reading + error + 2+ failed commands nearby → 0.9 confidence

### 2.4 Proactive Surfacing Protocol

**General rule:** Never interrupt without high confidence.

**Confidence thresholds:**
| Confidence | Action |
|------------|--------|
| ≥ 0.9 | Surface prominently (Mission Panel, badge) |
| 0.7–0.9 | Surface quietly (sidebar, collapsible) |
| 0.5–0.7 | Log intent, wait for confirmation |
| < 0.5 | Silent observation only |

**Operator controls:**
```yaml
# ~/.tamux/config.toml
[anticipatory]
enabled = true
morning_brief = true
morning_brief_window_minutes = 30
predictive_hydration = true
stuck_detection = true
stuck_detection_delay_seconds = 45
surfacing_min_confidence = 0.7
```

---

## Implementation Architecture

### 2.5 Background Tick Engine

Add a new background task type:
```rust
struct AnticipatoryTick {
    interval_secs: u64,     // default: 30
    last_session_activity: Instant,
    is_operator_present: bool,
}

impl AnticipatoryTick {
    fn tick(&mut self, state: &DaemonState) {
        // 1. Check session rhythm → morning brief?
        self.check_session_start(state);
        
        // 2. Check active goals → predictive hydration?
        self.check_goal_predictions(state);
        
        // 3. Check operator attention → stuck detection?
        self.check_attention_state(state);
        
        // 4. Update presence model
        self.update_operator_present(state);
    }
}
```

### 2.6 Context Pre-loader

```rust
struct ContextPreloader {
    queue: AsyncQueue<PreloadRequest>,
}

struct PreloadRequest {
    resource: ResourceRef,      // file path, URL, doc ID
    reason: PreloadReason,       // GoalPrediction | MorningBrief | Explicit
    priority: Priority,
    session_id: Option<SessionId>,
}
```

### 2.7 Surface Controller

```rust
struct SurfaceController {
    min_confidence: f64,
    last_surface: Instant,
    surface_cooldown: Duration,  // prevent spam
}

impl SurfaceController {
    fn should_surface(&self, item: &SurfaceItem) -> SurfaceDecision {
        if item.confidence < self.min_confidence {
            return SurfaceDecision::Skip;
        }
        if item.is_spam(&self.last_surface) {
            return SurfaceDecision::Defer;
        }
        // Route to appropriate panel based on M1 attention topology
        SurfaceDecision::Show { panel: self.target_panel(item) }
    }
}
```

---

## Wire Into Existing Modules

### `agent/liveness/health_monitor.rs`
Add anticipatory tick alongside health tick:
```rust
// Existing health tick (30s)
self.health_tick(state);

// New anticipatory tick (30s, independent)
self.anticipatory_tick.tick(state);
```

### `agent/context/archive.rs`
Add pre-loader integration:
```rust
// When archiving context, check if pre-loader predicted this
if let Some(prediction) = self.preloader.check_prediction(&item.key) {
    prediction.record_hit();
}
```

### `agent/metacognitive/self_assessment.rs`
Add attention state tracking:
```rust
// Track if operator seems stuck on current content
let attention_state = self.assess_attention(state);
if attention_state.is_stuck() {
    self.trigger_proactive_surface(attention_state);
}
```

---

## Milestones

- [ ] M2.1: Background tick engine — session start detection, interval loop
- [ ] M2.2: Morning brief card — surface unfinished goals, pending approvals, errors
- [ ] M2.3: Predictive context hydration — goal step context pre-loading
- [ ] M2.4: Attention state inference — terminal focus, reading time tracking
- [ ] M2.5: Stuck detection surface — hint surfacing with confidence gating
- [ ] M2.6: Operator controls — config file for timing, thresholds, disable
- [ ] M2.7: Surface cooldown — prevent notification spam

---

## Dependencies

- **Depends on:** M1 (session rhythm, attention topology, risk tolerance)
- **Enables:** None (terminal moat)
- **Conflicts:** None

---

## Test Plan

1. **Morning brief test**: Schedule 10 test sessions at different times, verify brief fires at correct times
2. **Predictive hydration test**: Run goal with known context needs, verify pre-loading
3. **Stuck detection test**: Simulate operator reading error, verify hint surface
4. **Calibration test**: Vary confidence thresholds, verify spam prevention
5. **Opt-out test**: Disable in config, verify no proactive behavior
