# tamux Memory Architecture

tamux memory is not a single file, a vector store, or a convenience summary pasted into the next prompt. It is a layered daemon-owned memory system that combines identity, durable facts, operator profile, workspace task and thread state, procedural memory, recall systems, provenance, and higher-order learning loops.

The shortest correct description is:

- the daemon owns memory
- memory is layered
- some layers are editable artifacts and some are structured state
- durable writes are curated, provenance-backed, and operator-auditable
- the system already has meaningful learning and recall surfaces
- the roadmap extends that foundation into episodic distillation, reflection, dream-state learning, and richer semantic memory

For the broader runtime architecture, see [how-tamux-works.md](./how-tamux-works.md). For governance, approvals, provenance integrity, and execution safety, see [tamux-security.md](./tamux-security.md).

## What Memory Means In tamux

In tamux, memory is the full persistence and recall substrate that lets work survive across turns, clients, agents, workspace tasks, and time.

That includes:

- stable persona identity
- durable project and environment facts
- operator profile and preference state
- persisted thread, workspace task, execution queue, and goal history
- retrieval over history and telemetry
- procedural memory encoded as skills
- structural and semantic workspace memory
- provenance-backed durable memory facts
- learned patterns from execution outcomes

If you describe tamux memory as “three markdown files,” you are describing only the most visible surface, not the actual system.

## Memory Is Daemon-Owned

The daemon is the authority for memory. Electron, the TUI, the CLI, MCP clients, and chat gateways are all clients of the same memory substrate.

That matters because:

- memory survives UI restarts
- multiple clients can reconnect to the same state
- handoffs and subagents can share common durable context
- learning loops can operate over persisted history instead of only current prompt state

This is one of the core differences between tamux and disposable terminal chat shells.

## The Memory Stack

tamux memory is best understood as a stack of layers rather than one store.

### 1. Identity Memory

`SOUL.md` is the identity layer.

It captures:

- the operating identity of a fire
- role, principles, and behavioral boundaries
- stable specialization hints

It is not meant to become a dumping ground for run output. It is the stable identity scaffold that shapes how an agent approaches work.

The iteration roadmap also pushes this layer forward:

- `.planning/iteration-2/09-agent-morphogenesis.md` describes adaptive specialization updates to `SOUL.md`
- the broader architecture treats identity as durable but not frozen forever

### 2. Durable Fact Memory

`MEMORY.md` is the explicit durable fact and strategy layer.

It is used for:

- project facts
- conventions
- stable environment knowledge
- learned workflow rules
- persistent corrections worth carrying forward
- strategy hints extracted from history

This is curated memory, not an event log.

The current system already treats `MEMORY.md` as a durable prompt input. The iteration specs extend it into a target for:

- episodic distillation via `[distilled]` entries
- reflection/forge strategy hints via `[forge]` entries
- dream-state strategy hints via `[dream]` entries

### 3. Operator Memory

`USER.md` is the operator-facing profile memory layer.

It is not just a manually edited note file. In tamux, operator memory is increasingly daemon-owned and structured:

- onboarding and check-ins are daemon workflows
- profile fields live in structured persistence
- `USER.md` is synchronized from that structured profile state
- approval behavior, revision habits, and other implicit signals can influence how the operator is modeled

This means operator memory is not only “what the user said once,” but also “what the daemon has durably learned about how this operator works.”

### 4. Working And Runtime Memory

Not all memory should become a markdown artifact. A large amount of memory lives as structured runtime state in SQLite and daemon-managed state.

This includes:

- agent threads and messages
- workspace task and execution queue state
- goal runs, steps, and reflections
- checkpoints
- work context
- collaboration sessions
- operator profile records
- causal traces
- implicit feedback and satisfaction records
- memory provenance records

This layer is what lets tamux resume work honestly after restarts and keep continuity across long-running goals.

### 5. Recall Memory

tamux has multiple retrieval paths rather than one generic “search memory” function.

Current recall surfaces include:

- `search_history` for direct history and transcript search
- `session_search` for grouped recall over messages, telemetry, and behavioral records
- `onecontext_search` when Aline history is available
- prompt-time injection of relevant summaries, operator-model state, causal guidance, and optional cross-session context

This matters because retrieval in tamux is contextual. Different questions need different memory surfaces.

### 6. Procedural Memory

Skills are procedural memory.

In tamux, procedural memory is not secondary. It is one of the strongest forms of reusable knowledge because it stores not just facts, but workable patterns of action.

That layer includes:

- local installed skills
- generated skills from successful trajectories
- skill variants
- usage and outcome tracking
- promotion, deprecation, archive, branch, and merge behavior

The roadmap deepens this further through the capability gene-pool design in `.planning/iteration-2/10-capability-gene-pool.md`.

### 7. Structural And Semantic Memory

tamux already has meaningful graph-like and semantic memory surfaces even though the full “Memory Palace” design is not completely shipped.

Current adjacent foundations include:

- semantic workspace querying over packages, imports, services, infra, conventions, and temporal workspace history
- per-thread structural memory carrying workspace seeds, observed files, and typed edges

The planned richer version is described in `.planning/iteration-2/17-semantic-memory-palace.md`:

- persistent memory nodes and edges
- graph navigation for multi-hop retrieval
- pruning, decay, clustering, and summary-node compression
- long-horizon structural retrieval beyond flat similarity search

The important point is that tamux is already past flat chat history, but the full cross-thread graph memory architecture remains an explicit direction for further growth.

### 8. Provenance-Backed Durable Memory

Durable memory writes are not “trust me, the file changed.”

tamux already tracks memory provenance so the system can answer:

- where a durable fact came from
- when it was written
- which thread, workspace task, execution queue entry, or goal produced it
- whether it has been confirmed or retracted
- which later operation explicitly invalidated or removed it

This is one of the major places where tamux memory is substantially stronger than a plain editable context file.

## Current Canonical Memory Surfaces

Today, the most important memory surfaces are:

- `SOUL.md`
- `MEMORY.md`
- `USER.md`
- persisted threads and messages
- workspace task, execution queue, and goal state
- structural workspace memory
- semantic environment state
- skill artifacts and skill metadata
- memory provenance and related audit state

Together these form the practical memory system tamux runs on now.

## The Memory Write Pipeline

tamux memory writes are curated.

At a high level, a durable write path looks like this:

1. A fact, preference, strategy hint, or other durable candidate emerges from a thread, workspace task, execution queue entry, goal reflection, operator profile change, or background learning pass.
2. The system decides which layer it belongs to.
3. The write is bounded and normalized instead of dumping raw history.
4. Contradiction or replacement rules are applied where relevant.
5. The durable artifact is updated.
6. Provenance metadata is persisted alongside the write.
7. Operator surfaces can later inspect, confirm, retract, or review that memory state.

This keeps memory useful instead of turning it into a transcript graveyard.

### Why Curation Matters

Curated memory is a design choice, not a limitation.

Without curation:

- memory bloats
- contradictions accumulate silently
- the prompt fills with low-signal residue
- agents learn the wrong habits from noise

tamux is deliberately trying to preserve durable signal rather than everything that happened.

## Compaction-Aware Memory Preservation

Memory and compaction are tied together.

When older context is about to be compacted, tamux can preserve durable signal before raw conversational detail falls out of the active window. The current architecture already includes pre-compaction preservation behavior, and the broader memory direction assumes compaction should not mean amnesia.

Practically, this means:

- important context should migrate into durable layers before it disappears from the active prompt
- long-lived threads benefit from stronger compaction strategy because preservation quality affects later recall quality
- LLM-backed compaction and memory curation are complementary, not competing mechanisms

## Provenance, Confidence, And Retraction

Durable memory in tamux is not just append-only prose.

The provenance-backed layer tracks:

- target file
- write mode
- source kind
- source scope such as thread, workspace task, queue entry, or goal when available
- extracted fact keys
- timestamps
- confirmation state
- retraction state
- explicit relationships such as `retracts`

This supports states such as:

- active
- uncertain
- confirmed
- retracted

This is a big part of why tamux memory can support operator trust instead of behaving like invisible prompt mutation.

## Learning Loops That Feed Memory

The roadmap makes memory much more than a static store. Several learning loops are intended to write back into durable memory in different ways.

### Episodic Distillation

`.planning/iteration-1/01-episodic-memory-distillation.md` defines distillation from older threads into durable candidates.

Its target output is:

- conventions
- corrections
- preferences
- patterns
- lessons

The design explicitly distinguishes confidence bands:

- high-confidence candidates can auto-apply
- medium-confidence candidates can queue for operator review
- low-confidence candidates are discarded

This is the core “conversation history becomes reusable memory” loop.

### Forge Reflection

`.planning/iteration-1/02-self-reflection-loop.md` pushes learning beyond thread text and into execution behavior.

This loop is meant to mine:

- tool fallback loops
- revision-triggering patterns
- timeout-prone behavior
- approval friction
- stale task accumulation

Its output is strategy memory, not factual memory.

That distinction matters:

- distillation learns from what was said
- forge learns from how the system actually performed

### Dream State

`.planning/iteration-2/07-dream-state.md` introduces offline counterfactual learning during idle periods.

This is the “what would have gone better if I had done X instead?” loop.

Dream-state memory is meant to:

- replay recent workspace tasks and execution entries
- evaluate counterfactual variations
- turn strong counterfactual wins into durable hints
- keep those hints auditable and removable

It is explicitly designed to run only when:

- there are no active sessions
- there are no pending workspace tasks or execution entries
- there are no active goal runs

### Implicit Feedback Learning

tamux already has real implicit-feedback behavior inside the operator-model stack, even if the larger standalone spec is only partially realized.

That layer already learns from signals such as:

- fast denials
- revision-style messages
- tool hesitation and fallback patterns
- rapid reverts after agent-authored edits
- attention and dwell-like signals

This affects memory because operator behavior becomes part of what the system remembers about how it should behave.

### Meta-Cognition And Resonance

The broader roadmap also connects memory to:

- meta-cognitive self-modeling
- cognitive resonance
- memory urgency adjustment
- future retrieval/prefetch decisions

These are not just “extra intelligence” features. They change when the system decides a fact should become durable, how urgently it should be preserved, and which historical knowledge should be surfaced now.

## Memory And The Multi-Agent Runtime

tamux memory is shared infrastructure, but not every layer is shared in the same way.

Examples:

- all fires can benefit from daemon-owned durable memory
- identity memory remains fire-specific
- operator memory is global to the operator relationship
- thread and task memory are scoped to their objects
- collaboration, handoff, and subagent flows create additional memory-bearing state

This matters because a multi-agent system cannot rely on one monolithic transcript. It needs scoped memory plus common durable context.

## Operator Surfaces

The memory system is visible through multiple surfaces, but those surfaces are not equally deep yet.

Current operator-facing behavior includes:

- markdown memory as prompt-bearing durable artifacts
- desktop Session Vault provenance views for durable memory inspection
- confirm/retract controls in richer desktop surfaces
- audit surfaces that expose related provenance summaries

Current limitations still exist:

- the TUI does not yet expose the full memory provenance interaction surface
- the full semantic-memory-palace graph is not yet a complete operator-facing product surface
- some advanced learning loops exist as roadmap architecture rather than fully shipped UI features

The important distinction is that the architecture is already beyond flat notes even when every future surface is not finished yet.

## Memory And Security

tamux memory is tightly tied to trust, but memory is not the same subsystem as governance.

The memory side focuses on:

- durable knowledge
- retrieval
- learning
- provenance of facts
- operator reviewability

The security side focuses on:

- whether side effects may happen
- what approvals are required
- how risk is classified
- how provenance integrity is enforced for execution
- how critique, containment, and compensation work

See [tamux-security.md](./tamux-security.md) for the governance and security plane.

## The Right Mental Model

The correct mental model for tamux memory is:

- identity memory
- fact memory
- operator memory
- runtime state memory
- retrieval memory
- procedural memory
- semantic/structural memory
- provenance-backed durable memory
- higher-order learning loops

This is why tamux can behave like a long-running operating environment rather than a fresh prompt window on every turn.

## Related Reading

- [how-tamux-works.md](./how-tamux-works.md)
- [tamux-security.md](./tamux-security.md)
- [self-orchestrating-agent.md](./self-orchestrating-agent.md)
- [goal-runners.md](./goal-runners.md)
- [orchestration-safety-architecture.md](./orchestration-safety-architecture.md)
