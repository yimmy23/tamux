# Layered Skill Selection And Packaging Design

Date: 2026-04-13
Status: Approved

## Summary

tamux skill discovery currently behaves like a ranked `top1` recommendation system. That is useful for finding a likely skill, but it is weaker than the multi-skill workflow behavior seen in Codex-style environments where the runtime can apply a small bundle of applicable skills for a turn. The current install/runtime story for built-in skills is also fragile because packaged installs do not ship the skills tree directly and daemon seeding depends on a repo-relative source path.

This design replaces the single-skill control flow with a layered selector that combines rule-triggered workflow skills and semantic ranking, introduces richer built-in skill metadata, and ships built-in skills as real files in release/install artifacts. Runtime skill storage remains a single merged `~/.tamux/skills` tree, while provenance and local-edit conflict tracking move into a manifest/ledger.

## Goals

- Make skill usage feel closer to Codex-style multi-skill guidance instead of `top1` ranking.
- Keep `required_skills` rare so normal agentic flows are not over-constrained.
- Use both deterministic triggers and semantic/LLM evidence when selecting skills.
- Add structured metadata/tags to built-in skills so they are easier to rank and inspect.
- Ship built-in skills as actual files in release/install packages.
- Preserve local edits to built-in skills across upgrades in a merged runtime tree.
- Unify daemon, CLI, TUI, MCP, npm installs, and script-based installs around one canonical runtime skill root.

## Non-Goals

- Building a full policy DSL for skill selection in the first rollout.
- Forcing every non-trivial turn to read multiple skills.
- Splitting built-in and user skills into separate runtime directories.
- Replacing the existing discovery stack with LLM-only selection.

## Problems

### 1. Discovery collapses to one action

The current runtime may rank multiple candidates, but the control path still collapses to a single `recommended_action` and effectively one top skill. Weak matches also collapse into `read_skill <top-skill>`, which makes the system feel top1-shaped even when several skills are relevant.

### 2. Strong workflow skills are under-expressed

Codex-style sessions often combine process skills like `brainstorming`, `systematic-debugging`, and `test-driven-development` because the environment has explicit trigger rules. tamux currently relies too heavily on textual ranking to rediscover these workflows each time.

### 3. Built-in skill metadata is inconsistent

Many built-in skills lack strong structured metadata, which makes categorization and ranking too dependent on filenames and body prose.

### 4. Packaged install behavior is fragile

npm and direct-install flows ship binaries but not the skill tree. The daemon currently seeds built-in skills from a compile-time repo-relative source path, which is fine in a source checkout but not a reliable contract for packaged installs.

### 5. Runtime roots are not fully canonicalized

The daemon currently prefers `agent/skills` if it exists and otherwise falls back to `~/.tamux/skills`, while MCP reads `~/.tamux/skills` directly. That asymmetry increases the risk of catalog drift.

## Product Decisions

- Selection uses both deterministic triggers and semantic/LLM ranking.
- Triggered skills use strong guidance by default, not hard gates.
- `required_skills` should be uncommon and reserved for high-confidence situations.
- Built-in skills are shipped as actual files in release/install artifacts.
- Runtime storage remains one merged `~/.tamux/skills` tree.
- Local edits to shipped built-ins are preserved using conflict tracking rather than overwrite-on-upgrade.

## Architecture

## 1. Layered Skill Selector

Replace the single-skill recommendation contract with a layered bundle selector.

The discovery result for a turn becomes:

- `required_skills`
- `guided_skills`
- `suggested_skills`
- `triggered_skills`
- `semantic_skills`
- `primary_skill`
- `read_sequence`
- `skip_rationale_required_for`
- `selection_reasons`

### Selection pipeline

1. A rule pass proposes workflow/process skills from high-confidence signals.
2. A metadata/lexical/semantic/LLM pass ranks the installed catalog.
3. A combiner merges both sources, deduplicates, and caps the total bundle.
4. A classifier assigns each selected skill to `required`, `guided`, or `suggested`.

### Guidance model

- `required_skills`
  - Rare.
  - Used only when confidence is extremely high and the workflow is broadly applicable.
  - Skipping requires explicit rationale and should be unusual.

- `guided_skills`
  - Default strong-guidance set.
  - These are expected reads first, but the model may skip with rationale.

- `suggested_skills`
  - Optional supporting skills.
  - Helpful when they materially improve the turn, but not expected every time.

### Expected bundle sizes

Most turns should look like:

- `required_skills: []`
- `guided_skills: 1-2`
- `suggested_skills: 0-2`

`read_sequence` should usually be capped at 3 skills max to prevent over-reading.

## 2. Rule Triggers Plus Semantic Confirmation

Rule triggers should propose candidates, not fully dictate execution.

Examples:

- debugging, failures, broken tests, crashes
  - propose `systematic-debugging`
- implementation, bugfix, feature work
  - propose `test-driven-development`
- design, feature shaping, behavioral changes
  - propose `brainstorming`

These proposals must then be confirmed, boosted, demoted, or dropped by semantic evidence. This avoids over-triggering process skills during general agentic work.

## 3. Structured Skill Metadata

Built-in skills should gain normalized frontmatter fields for stronger categorization and ranking:

- `domains`
- `languages`
- `ecosystems`
- `intent_tags`
- `phase_tags`
- `risk_tags`
- `trigger_keywords`

These fields should be parsed into the skill catalog and exposed across daemon, CLI, TUI, and MCP.

Ranking should combine:

- trigger matches
- frontmatter metadata overlap
- lexical overlap
- semantic similarity
- historical success/use signals
- workspace/context tag overlap

## 4. Single Merged Runtime Tree With Conflict Tracking

The canonical runtime tree remains:

- `~/.tamux/skills/...`

Built-in, community, generated, and user-edited skills all live in this merged tree. Provenance and conflict state are stored outside the content tree in a ledger/manifest.

Each tracked skill record should store at least:

- `source_kind` (`builtin`, `community`, `generated`, `user`)
- `source_package_version`
- `source_relative_path`
- `installed_path`
- `shipped_hash`
- `installed_hash`
- `locally_modified`
- `last_sync_at`
- `conflict_state`

### Upgrade behavior

- If a shipped built-in is unchanged locally, upgrade replaces it in place.
- If a shipped built-in was locally modified, keep the local file and mark a conflict.
- Conflict state should be visible via skill inspection/maintenance surfaces.

## 5. Packaged Built-In Skills

Release/install artifacts must ship the built-in skill tree as actual files.

That means:

- release artifacts include a `skills/` bundle
- npm install flow installs both binaries and built-in skills
- direct shell/PowerShell installers install both binaries and built-in skills

The daemon should no longer depend on a repo-relative source path as the primary packaged-install source of truth. Repo seeding may remain as a source-checkout fallback for developers, but it should not be the packaging contract.

## 6. Canonical Runtime Root Resolution

All consumers should resolve the same runtime skill root using one shared policy.

Current asymmetry between daemon and MCP should be removed. The preferred contract is:

- one canonical runtime root for installed skills: `~/.tamux/skills`
- no hidden divergence between `agent/skills` and `~/.tamux/skills` in packaged installs

If internal staging directories are needed, they should not change what discovery surfaces read as the installed catalog.

## Runtime Prompt Contract

Prompt guidance should stop describing discovery as a top-match workflow and instead describe it as a selected bundle for the turn.

The model should be told:

- read `required_skills` first
- strongly prefer `guided_skills`
- use `suggested_skills` when they materially help
- record rationale when skipping a `required` or clearly relevant `guided` skill

This changes compliance from:

- "read the top match"

to:

- "follow the selected skill bundle for this turn"

## Data Model Changes

New/extended structures should include:

- skill metadata fields for structured tags
- provenance/conflict ledger rows
- bundle-based discovery result fields
- telemetry fields for:
  - selected bundle
  - reads performed
  - skipped guided skills
  - skipped required skills
  - final task outcome

## Rollout Plan

### Phase 1: packaging and canonical roots

- Ship built-in skills in release assets.
- Update npm and direct installers to install the skill tree.
- Canonicalize runtime root resolution across daemon, CLI, TUI, and MCP.
- Keep repo-relative seeding only as a source-checkout fallback.

### Phase 2: metadata normalization

- Add structured frontmatter/tags to built-in skills.
- Add validators so built-ins cannot drift into inconsistent metadata.
- Reindex installed skills into the richer catalog model.

### Phase 3: layered selector

- Add rule proposals plus semantic bundle selection.
- Emit both legacy top-skill fields and new bundle fields temporarily for compatibility and comparison.
- Add telemetry to compare legacy and new behavior.

### Phase 4: prompt and compliance switch

- Update runtime prompt wording to bundle-based guidance.
- Change compliance logic to operate on `required` and `guided` sets instead of only a top skill.
- Remove or de-emphasize legacy top1 behavior after confidence is high.

## Validation

Validation must cover both packaging and runtime behavior.

### Install and packaging

- source-checkout run still works
- npm install installs built-in skills into the canonical runtime tree
- direct installer installs built-in skills into the canonical runtime tree
- upgraded installs preserve local edits and mark conflicts correctly

### Catalog and metadata

- shipped built-ins are visible in the runtime tree
- metadata is parsed consistently
- built-in tag validators catch malformed or incomplete frontmatter

### Discovery behavior

- triggered and semantic skills merge correctly
- normal agentic flows do not overproduce `required_skills`
- debugging and implementation flows consistently surface the right `guided_skills`
- bundle size stays capped and reasonable

### Prompt and compliance

- prompt includes bundle fields
- `required` and `guided` compliance behave as designed
- skip rationale is enforced only where appropriate

## Risks

- Over-triggering process skills could make normal flows rigid.
- Under-tagged or inconsistently tagged built-ins could create false confidence in the selector.
- Merged-tree conflict handling adds upgrade complexity and requires careful operator-facing inspection.
- Supporting both legacy and new discovery contracts temporarily increases code-path complexity.

## Recommendation

Implement the layered selector with metadata-backed ranking and packaged built-in skills, but keep `required_skills` intentionally rare. The system should optimize for strong guidance rather than heavy-handed enforcement, because tamux handles many normal agentic flows that should stay flexible.
