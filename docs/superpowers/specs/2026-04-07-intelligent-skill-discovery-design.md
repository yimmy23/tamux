# Intelligent Skill Discovery Design

## Summary

tamux currently exposes local skills through `list_skills`, but the agent experience is still catalog-oriented rather than decision-oriented. Agents often consult the tool as a lightweight read, then continue without treating skills as the authoritative workflow for the task at hand.

This design introduces a daemon-enforced skill discovery system that:

- makes installed-skill discovery mandatory before non-trivial work,
- ranks skills by reliability and task fit instead of dumping a plain list,
- records whether the agent complied with the recommendation or explicitly skipped it,
- upgrades CLI, TUI, Electron, and MCP surfaces to expose the same ranked discovery result,
- and optionally launches a non-blocking community-skill scout in the background when enabled by operator settings.

The main goal is behavioral: agents should search for, select, and follow a tailored skill before acting, not merely acknowledge that skills exist.

## Problem

The current local skill flow has four practical weaknesses:

1. `list_skills` returns a mostly flat wall of text.
2. Matching is shallow and requires the model to infer too much from names, paths, and unrelated files.
3. There is no hard compliance check tying discovery to later tool execution.
4. Human-facing surfaces do not expose ranked recommendations, confidence, or why a skill was chosen.

tamux already has useful building blocks:

- daemon-tracked skill variants with lifecycle state, tags, use counters, and success counters,
- workspace-context tag inference,
- a `skill_preflight` scorer that already ranks candidate skills from request text and context,
- MCP and CLI surfaces for listing and inspecting skills,
- audit and workflow-notice plumbing that can record skill consultation.

The missing piece is a first-class discovery subsystem that converts those signals into a required pre-action step with structured output and enforceable compliance.

## Goals

- Require skill discovery before non-trivial agent work.
- Recommend the best installed skill with concise reasons and confidence.
- Force either `read_skill` compliance or an explicit skip rationale when no strong fit exists.
- Keep community discovery asynchronous and non-blocking.
- Share ranking logic across daemon runtime, MCP, CLI, TUI, and Electron.
- Preserve backward compatibility for raw skill listing and inspection.

## Non-Goals

- Replacing local installed skills with remote/community skills for the current turn.
- Building a full vector search stack in the first rollout.
- Auto-importing community skills without operator approval.
- Preventing trivial turns from proceeding when no skill is relevant.

## External Research

Two retrieval lessons shape this design:

1. Hybrid retrieval beats a single strategy. Anthropic's Contextual Retrieval write-up argues that lexical signals plus semantic/contextual signals plus reranking substantially reduce retrieval misses, especially when exact terms and broader intent both matter.
2. Tool and skill selection should be treated as a first-class agent capability, not passive prompt text. Current OpenAI developer docs separate tools, skills, and tool search as distinct system capabilities, which aligns with making discovery explicit in tamux rather than leaving it as best-effort prompt wording.

Sources:

- Anthropic, "Contextual Retrieval": https://www.anthropic.com/engineering/contextual-retrieval
- OpenAI Developers portal: https://developers.openai.com/

## Product Decisions

The approved design decisions for this feature are:

- Discovery is a hard gate for non-trivial work.
- The system should prioritize installed local skills.
- Community skill discovery runs in the background and is non-blocking.
- Community import uses a short operator approval prompt and should be configurable in advanced settings.
- When no strong local skill is found, the agent may proceed only after recording an explicit rationale.
- Enforcement is hybrid: prompt guidance plus daemon runtime enforcement.

## User Experience

### Agent Runtime

For non-trivial incoming turns, the daemon runs discovery before allowing substantial tool execution.

The agent sees a structured recommendation such as:

- recommended skills,
- confidence tier,
- concise reasons,
- expected next step,
- compliance requirements.

If confidence is strong, the expected next step is to call `read_skill` on the top match before other substantial actions.

If confidence is weak or none, the agent may proceed only after emitting an explicit structured rationale that no installed skill is suitable.

### MCP

MCP gains a dedicated discovery tool for recommendation-oriented usage. `list_skills` remains available as raw catalog output for manual inspection and backward compatibility.

### CLI

The CLI gains a recommendation-oriented command, for example:

```bash
tamux skill discover "debug intermittent tui render panic"
```

The output shows:

- ranked matches,
- confidence,
- reasons,
- and recommended next action.

### TUI And Electron

The operator should see:

- whether a turn was skill-gated,
- which skill was recommended,
- whether the agent complied,
- and whether the agent explicitly skipped the recommendation.

Advanced settings gain controls for:

- enabling background community-skill discovery,
- enabling the short approval prompt,
- and learnable promotion to a global default after repeated approvals.

## Architecture

### 1. Shared Discovery Service

Create a daemon-owned discovery module that centralizes:

- request normalization/tokenization,
- candidate loading,
- scoring,
- confidence calculation,
- recommendation formatting,
- and compliance-state generation.

This module becomes the single source of truth for:

- agent preflight,
- CLI `skill discover`,
- MCP `discover_skills`,
- TUI/Electron recommendation views,
- and future community-scout ranking integration.

The existing logic in `crates/amux-daemon/src/agent/skill_preflight.rs` should be moved or generalized into this shared layer rather than duplicated.

### 2. Hard-Gate Runtime Enforcement

Add a per-turn skill-discovery checkpoint to the daemon agent loop.

For each incoming turn:

1. Classify whether the request is trivial or non-trivial.
2. If trivial, skip gating.
3. If non-trivial, run local discovery before normal tool execution.
4. Persist a structured discovery record on the thread/turn.
5. Before allowing substantial tool execution, verify compliance:
   - top recommendation was read, or
   - explicit skip rationale was recorded for weak/none confidence.

If compliance is missing, inject the discovery result into the prompt/tool flow and block ordinary execution until the agent resolves it.

### 3. Background Community Scout

If advanced settings enable it, the daemon spawns a background community-skill scout after local discovery.

Properties:

- never blocks the current turn,
- never replaces the local installed-skill gate,
- may surface a short operator approval prompt,
- may learn from repeated approvals and suggest a global default,
- records results separately from local-skill compliance.

The local turn proceeds according to installed-skill results only.

## Discovery Model

### Candidate Inputs

Each installed skill candidate should be ranked using:

- `skill_name`,
- `variant_name`,
- relative path,
- extracted summary/description,
- extracted trigger phrases,
- extracted domain keywords,
- extracted tool families,
- `context_tags`,
- lifecycle status,
- use count,
- success count,
- failure count,
- recency,
- current workspace tags,
- current turn text.

### Metadata Extraction

The daemon should parse and persist richer metadata from `SKILL.md` files at registration or refresh time. Initial extracted fields:

- title/name,
- description,
- when-to-use text,
- keywords/verbs,
- domain tags,
- tool-family hints,
- prerequisites,
- safety/risk hints.

This metadata should be refreshed when the file changes and stored alongside existing variant records or in a companion table.

### Ranking Signals

The local scorer should combine:

- lexical overlap between request text and searchable skill metadata,
- workspace-context overlap,
- lifecycle bonus for canonical/active/proven variants,
- reliability based on success/failure history,
- recency bonus,
- optional built-in fallback bonuses for general workflow skills,
- family diversity so the result set does not collapse into near-duplicate variants.

This is a hybrid retrieval model using current repository-friendly signals first. It leaves room for a later semantic reranker without changing the outer interface.

### Confidence Tiers

The scorer should output both raw scores and a coarse confidence tier:

- `strong`: clear top fit, small ambiguity, enough reliability
- `weak`: plausible options exist, but fit or reliability is uncertain
- `none`: no candidate crosses a minimum threshold

The tier determines runtime policy:

- `strong`: read the top skill before substantial action
- `weak` or `none`: agent may proceed only with explicit skip rationale

## Compliance And Audit

Add explicit discovery/compliance state to the thread or turn:

- discovery required,
- discovery executed,
- candidates returned,
- recommended skill,
- confidence tier,
- skill actually read,
- skip rationale,
- community scout launched,
- timestamps.

This state should feed:

- workflow notices,
- audit views,
- TUI/Electron activity panels,
- future evaluation of agent discipline.

The important property is traceability: tamux should be able to answer whether the agent searched, what it found, and whether it followed the recommendation.

## API And Protocol Changes

### Daemon Protocol

Add a recommendation-oriented client/server message pair, for example:

- `ClientMessage::SkillDiscover { query, session_id, limit }`
- `DaemonMessage::SkillDiscoverResult { result_json }`

The result should be structured JSON, not plain text, so all clients can render the same data.

### MCP

Add a new tool:

- `discover_skills`

Suggested arguments:

- `query`: required request text
- `limit`: optional result cap
- `session_id` or equivalent context hint when available

Keep `list_skills` for raw listing and `read_skill` for consumption.

### CLI

Add:

- `tamux skill discover <query>`

Keep:

- `tamux skill list`
- `tamux skill inspect`

This cleanly separates discovery from catalog browsing.

## UI Surfaces

### TUI

Add a recommendation view or status panel content showing:

- confidence tier,
- top matches,
- reasons,
- compliance outcome,
- community scout state if enabled.

### Electron/React

Add a corresponding recommendation component to agent/task activity surfaces and advanced settings toggles for community discovery behavior.

## Implementation Plan Shape

The implementation should be sequenced as:

1. Shared discovery types and scoring service in the daemon.
2. Daemon protocol and MCP discovery tool.
3. Agent loop enforcement and compliance persistence.
4. CLI `skill discover`.
5. TUI and Electron recommendation surfaces.
6. Background community scout plus advanced settings.
7. Evaluation and tuning of scoring thresholds.

## Risks

### False Positives

An aggressive threshold could force irrelevant skills into the loop.

Mitigation:

- conservative `strong` threshold,
- explicit `weak` tier,
- operator-visible reasons,
- threshold tuning from real traces.

### False Negatives

A useful skill may be missed if metadata is poor.

Mitigation:

- richer extracted metadata,
- hybrid lexical/context scoring,
- later semantic reranking behind the same interface.

### Friction

A hard gate can slow agent turns if over-applied.

Mitigation:

- classify trivial vs non-trivial turns,
- keep discovery lightweight,
- only require explicit rationale when confidence is weak/none.

## Open Questions

- Whether metadata should live in the existing variant table or a companion index table.
- Which exact turn classes count as trivial for bypassing the gate.
- Whether the compliance record should be turn-scoped only or also summarized at thread scope.
- How the 30-second approval prompt should degrade when a client surface is disconnected.

## Acceptance Criteria

- Non-trivial turns trigger local skill discovery before substantial agent action.
- The agent cannot continue without either reading a strongly recommended skill or recording an explicit no-suitable-skill rationale.
- CLI, MCP, TUI, and Electron consume the same structured discovery result.
- `list_skills` remains available for raw inspection.
- Background community scout is optional, non-blocking, and operator-controlled.
- Audit/workflow views show recommendation and compliance state.
