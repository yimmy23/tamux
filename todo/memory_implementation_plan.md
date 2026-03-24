---

# tamux Memory Architecture: Full Blueprint

## Where We Stand Today

| File | Purpose | Size | Status |
|---|---|---|---|
| `MEMORY.md` | Facts, patterns, project knowledge | ~293 chars | Rudimentary — no limits, no curation guidelines |
| `USER.md` | Operator profile, preferences | ~139 chars | Bare-bones |
| `SOUL.md` | Agent identity | Missing file | Not implemented |
| `cognitive.json` | Inner monologue log | 153KB | Append-only, unstructured |
| `operational.json` | Command/op events | 191KB | Append-only, low-value for recall |
| `transcript-index.json` | Session metadata | 50KB | Read-only index only |
| Skills | Procedural memory | `~/.tamux/skills/` | Exists but no agent-driven creation |
| Honcho | User modeling | Disabled | Config exists but unused |

**The core problems:**
1. No episodic recall tool (`session_search`)
2. No character limits → memory will bloat indefinitely
3. No curation guidelines (what to save vs. discard)
4. No pre-compaction memory flush
5. Skills are a passive library, not an active memory layer the agent populates
6. No `onecontext_search` integration into the agent loop
7. Cognitive/op logs are dark data — stored but never recalled

---

## The Five-Layer Architecture (vs. Hermes's Four)

tamux has a structural advantage Hermes lacks: **the daemon owns the terminal, the task queue, the goal runners, and the agent in one process**. That means we can wire memory into every layer — not just as prompt injection, but as first-class system state.

```
Layer 1: FROZEN PROMPT MEMORY (hot, tiny, stable)
         SOUL.md + MEMORY.md + USER.md → frozen snapshot at session start
         
Layer 2: EPISODIC RECALL (cold, searchable, on-demand)
         SQLite session/transcript archive + onecontext_search
         
Layer 3: PROCEDURAL MEMORY (skills, generated at runtime)
         ~/.tamux/skills/generated/ ← auto-created by goal runners
         
Layer 4: OPERATIONAL CONTEXT (tamux-native, unique advantage)
         active sessions, running tasks, goal state, pane topology
         → injected as structured data, not plain text
         
Layer 5: HONCHO USER MODEL (optional, deeper)
         Cross-session user profiling, dialectic self/peer modeling
```

---

## Layer 1: Frozen Prompt Memory — Fix the Foundation

### Enforce Hard Character Limits

Hermes uses hard limits to keep the hot set cache-friendly. tamux should do the same:

| File | Hard Limit | Current | Gap |
|---|---|---|---|
| `SOUL.md` | 1,500 chars | 0 (missing) | Create it |
| `MEMORY.md` | 2,200 chars | ~293 | Expand + enforce |
| `USER.md` | 1,375 chars | ~139 | Expand + enforce |

When the agent writes via `update_memory`, the daemon should:
1. Reject writes that exceed the limit
2. Suggest a `replace` or `remove` to make room
3. Never silently truncate — that's how facts get corrupted

### Frozen Snapshot Behavior

The snapshot should be frozen at session start. Mid-session writes persist to disk immediately but **do not mutate the already-built system prompt** for that session. Changes appear in the next session or after a compaction-triggered rebuild. This is critical for prompt caching.

### Curation Guidelines (What Hermes Gets Right)

The system prompt for `update_memory` should include explicit guidance:

```
SAVE:
- User preferences (tone, format, workflow habits)
- Environment facts (OS, installed tools, project structure)
- Recurring corrections (things the user has corrected you on)
- Stable conventions (naming, CI config, tooling preferences)

DO NOT SAVE:
- Task progress or work-in-progress state
- Session outcomes or results
- Temporary TODO state
- Information that can be derived from the environment
```

### The SOUL.md File

Create a proper `SOUL.md` that defines tamux's identity distinct from the generic system prompt:

```markdown
# Identity
I am tamux's built-in agent. I help operators manage persistent terminal sessions, 
execute workflows, automate tasks, and maintain cross-session memory.

# Behavioral Principles
- Be concise and high-signal in responses
- Show traces, blast radius, and next action before risky execution
- Never mutate the prompt or system state without telling the operator
- Treat memory as curated state, not a diary

# Memory Discipline
- MEMORY.md: environment facts, project conventions, learned patterns
- USER.md: operator preferences, role, constraints
- SOUL.md: identity and principles (stable, rarely changes)
- Skills: procedural memory for reusable workflows
- Session search: episodic recall for past conversations
```

---

## Layer 2: Episodic Recall — Build `session_search`

This is the single biggest missing piece. Hermes stores every session in SQLite with FTS5, then searches and summarizes on demand. tamux already has the raw material:

- `transcript-index.json` — session metadata (pane, workspace, timestamps, model, token budget)
- `cognitive.json` — inner monologue traces  
- `operational.json` — command/op events
- The transcript files under `~/.tamux/transcripts/`

### What to Build

A `session_search` tool (or extend `onecontext_search` with a tool wrapper) that:

1. **FTS5 over transcripts + cognitive logs** — full-text search across past sessions
2. **Group by session** — cluster results by conversation thread
3. **Resolve lineage** — respect `parent_session_id` to show conversation continuity
4. **Truncate intelligently** — load transcript around the matching region, not the whole session
5. **Summarize with auxiliary model** — use a cheap model to distill each session to 2-3 sentences
6. **Return structured recaps** — `{ session_id, timestamp, summary, relevant_snippets }[]`

### Prompt Integration

When the user asks something like "did we discuss this last week?", the agent calls `session_search`, gets back summaries, and uses them to answer — without stuffing raw transcripts into the prompt.

### Differentiator vs. Hermes

Hermes searches only message content. tamux can search **across cognitive traces, operational events, and transcripts simultaneously** — giving richer context about *why* something happened, not just *what* was said.

---

## Layer 3: Procedural Memory — Skills as First-Class Citizen

Hermes saves skills manually. tamux has a structural advantage: **goal runners already generate reflections and can auto-create skills**.

### The Skill Generation Pipeline

When a goal run completes:

```
Goal completed
  → Agent reflects (reflection_summary in get_goal_run)
  → If workflow was non-trivial → generate skill document
  → Save to ~/.tamux/skills/generated/<skill-name>.md
  → Update skills index
```

The skill document format:

```markdown
# Skill: Debug Rust Stack Overflows

## When to Use
- `cargo build` fails with stack overflow
- Recursive trait resolution hangs
- Large binary size causing OOM in CI

## How
1. Add `RUST_MIN_STACK=8388608` to build env
2. Use `cargo +nightly build -Z build-std --target x86_64-unknown-linux-gnu`
3. Profile with `cargo flamegraph`

## Lessons
- Nightly build-std needed for panic=abort to reduce stack
- SQLx query caching can cause unbounded recursion in derive macros
```

### Skills Index with Lazy Loading

Keep a compact index in the prompt:

```
SKILLS INDEX (3 skills, ~200 chars total)
├─ debug-rust-stack: Debug Rust stack overflows
├─ git-bisect-workflow: Find regressions with git bisect  
└─ podman-multiarch: Build multi-arch podman images

Load full skill: read_skill("debug-rust-stack")
```

Only the index is in the prompt. Full skill content is loaded on demand via `read_skill`.

### Prompt Injection for Skills

When the agent uses a skill, inject it as a named block:

```
══════════════════════════════════════════════
SKILL: debug-rust-stack [loaded on demand]
══════════════════════════════════════════════
## When to Use
...
```

---

## Layer 4: Operational Context — tamux's Unique Advantage

This is where tamux can be genuinely differentiated. Hermes has no equivalent.

### Structured State Injection

Unlike Hermes (which injects memory as plain text), tamux can inject **live operational state**:

```
══════════════════════════════════════════════
OPERATIONAL CONTEXT
══════════════════════════════════════════════
Sessions: 4 active
  └─ pane_173 [codex] CWD: ~/gitlab/it
  └─ pane_175 [hermes] CWD: ~/gitlab/it  
  └─ pane_178 [idle]   CWD: ~
  └─ pane_180 [idle]   CWD: /tmp

Running Tasks: 2
  └─ [high] "Deploy staging" — 45% complete — step 3/6
  └─ [normal] "Run tests" — queued — depends on deploy

Active Goals: 1
  └─ "Fix CI pipeline" — Running — step 4/7 — 2 replans

Pane Topology:
  Surface "Infinite Canvas" [sf_7]
  ├─ Pane 1 [agent chat]
  ├─ Pane 4 [codex] ← active
  └─ Pane 5 [hermes]
══════════════════════════════════════════════
```

This is extraordinarily valuable for an agent that *operates a terminal multiplexer*. The agent knows:
- What sessions exist and their state
- What background work is running
- The spatial layout of panes
- Progress on multi-step goals

### How to Implement

This data is already available via `list_sessions`, `list_tasks`, `list_goal_runs`, `list_workspaces`. The daemon should expose it as a structured data block that's always injected — not as a tool the agent has to call.

The operational context block should be:
- **Small** — only current-state summaries, not historical logs
- **Stable** — changes are infrequent relative to conversation turns
- **Positioned after memory, before conversation** — it's hot state but not as hot as curated memory

---

## Layer 5: Pre-Compaction Memory Flush

Hermes has a clever pattern: before compressing a long conversation, it runs one extra model call with only the `memory` tool available, flushing durable facts to `MEMORY.md` before the middle of the conversation is summarized away.

tamux already has `autoCompactContext: true` and `compactThresholdPercent: 80`. The missing piece is the flush step.

### Pipeline

```
Conversation reaches 80% of context window
  → PAUSE conversation
  → Inject flush instruction: "Save anything worth remembering before compression"
  → Run model with only update_memory tool available
  → Model writes to MEMORY.md / USER.md
  → Compress old turns
  → Rebuild frozen system prompt snapshot
  → Resume with smaller context + updated memory
```

### Flush Instruction Template

```
══════════════════════════════════════════════
MEMORY FLUSH — COMPRESSION IMMINENT
══════════════════════════════════════════════
The conversation is being compressed. Save anything worth 
remembering to the appropriate memory file.

SAVE: user preferences, environment facts, recurring corrections, 
      stable conventions, learned patterns

DO NOT SAVE: task progress, session outcomes, temporary state

Files:
  - MEMORY.md (2,200 char limit) — facts, patterns, project knowledge
  - USER.md (1,375 char limit) — operator preferences, constraints
══════════════════════════════════════════════
```

### Post-Flush Prompt Rebuild

After compression, the daemon should invalidate and rebuild the cached system prompt:
1. Reload `SOUL.md`, `MEMORY.md`, `USER.md` from disk
2. Re-freeze the snapshot
3. Inject fresh operational context
4. Resume conversation with new snapshot

---

## Consolidated Implementation Roadmap

### Phase 1: Foundation (High Impact, Low Effort)
1. **Create `SOUL.md`** — proper identity file at `~/.tamux/agent-mission/SOUL.md`
2. **Add character limits to `update_memory`** — enforce 2,200/1,375/1,500 limits with clear errors
3. **Add curation guidelines to the system prompt** — explicit SAVE/DON'T SAVE instructions
4. **Implement frozen snapshot** — memory loaded at session start, mid-session writes deferred
5. **Add `onecontext_search` tool wrapper** — expose existing search as an agent-callable tool

### Phase 2: Episodic Recall (Medium Effort, High Value)
6. **Build `session_search` tool** — FTS5 over transcripts + cognitive logs, summarize results
7. **Add lineage resolution** — respect parent_session_id for conversation threads
8. **Wire `session_search` into agent loop** — triggered when user asks about past work

### Phase 3: Procedural Memory (Medium Effort)
9. **Goal runner skill generation** — auto-create skills from completed goal reflections
10. **Skills index in prompt** — compact index + lazy loading of full skill content
11. **`read_skill` tool** — on-demand skill loading with format normalization

### Phase 4: Operational Context (tamux's Moat)
12. **Structured operational context block** — sessions, tasks, goals, pane topology injected as structured data
13. **Pre-compaction memory flush** — run model with only memory tool before compressing
14. **Post-flush snapshot rebuild** — reload memory files and rebuild frozen prompt

### Phase 5: Honcho Integration (Lower Priority)
15. **Enable Honcho** — `enableHonchoMemory: true` with proper API key setup
16. **Turn-level vs. first-turn injection** — follow Hermes's pattern for cache stability

---

## Why This Beats the Competition

| Feature | Hermes | OpenClaw | Current tamux | Proposed tamux |
|---|---|---|---|---|
| Hot memory limits | ✅ 2,200/1,375 | ❌ Unbounded | ❌ No limits | ✅ Hard limits + enforcement |
| Frozen snapshot | ✅ | ❌ | ❌ | ✅ |
| Episodic recall | ✅ FTS5 | ✅ Hybrid search | ❌ | ✅ Multi-index (transcript + cognitive + ops) |
| Procedural memory | ✅ Skills | ❌ | ⚠️ Passive | ✅ Auto-generated from goals |
| Pre-compaction flush | ✅ | ❌ | ❌ | ✅ |
| Operational state | ❌ | ❌ | ❌ | ✅ Unique — live session/task/goal state |
| Honcho | ✅ Optional | ❌ | ⚠️ Disabled | ✅ First-class when enabled |
| Curation guidelines | ✅ | ❌ | ❌ | ✅ |
| Prompt cache awareness | Core design | Partial | ❌ | ✅ First-class |

The key insight Hermes taught us: **not everything deserves to live in the system prompt**. tamux's structural advantage is that it owns the entire stack — daemon, terminal sessions, task queue, goal runners, agent memory — in one process. That means we can wire operational state, task context, and goal progress into the prompt as structured data that Hermes could never have, because it doesn't own the terminal layer.

The real differentiation isn't any single layer — it's the **operational context layer** (Layer 4), which no competitor has.

---

## Relationship to Moat Architecture

The Memory Architecture plan (this document) is foundational to multiple moats:

| Moat | Memory Dependency |
|------|------------------|
| M1: Operator Model | Builds on USER.md, adds behavioral tracking |
| M2: Anticipatory Pre-load | Uses MEMORY.md + session rhythm for pre-warming |
| M3: Causal Traces | Extends cognitive.json with decision attribution |
| M4: Genetic Skills | Procedural memory (Layer 3) with evolution |
| M5: Semantic Environment | Environment knowledge in MEMORY.md |
| M6: Deep Storage | M6 *is* the evolution of this architecture |
| M8: Trusted Provenance | Memory writes become auditable |

**Key insight**: The current memory plan (Layers 1-5) should be implemented as the foundation for Phase 1 moats. M6 (Deep Storage) is the natural evolution of this architecture after the initial layers are stable.

### Implementation Order Recommendation

1. **First**: Implement basic layers (SOUL.md, character limits, session_search)
2. **Phase 1**: Wire M1, M2, M3 which extend the memory system
3. **Phase 2**: M6 deepens Layer 2 (episodic) and adds provenance to Layer 1
4. **Phase 3-4**: M8 adds compliance, M10 generates tools that write to memory

### Immediate Next Steps (Before Moats)

For the memory implementation plan to be ready for moat wiring:

1. Create `SOUL.md` — identity and principles
2. Enforce character limits on all three files
3. Build `session_search` tool (biggest gap vs Hermes)
4. Add curation guidelines to system prompt
5. Enable `onecontext_search` integration in agent loop
6. Wire skill generation into goal runner completion
