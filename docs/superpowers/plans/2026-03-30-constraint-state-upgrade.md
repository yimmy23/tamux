# Constraint-State Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade episodic negative knowledge so constraints carry explicit lifecycle state, simple provenance, and one-hop propagation while staying backward compatible with the existing daemon architecture.

**Architecture:** Extend the existing `NegativeConstraint` model in `amux-daemon` instead of creating a new graph subsystem. Add additive schema fields, pure helper functions for normalization/state/propagation, then thread the richer model through persistence and prompt formatting with focused unit tests at each stage.

**Tech Stack:** Rust, rusqlite, serde/serde_json, existing `amux-daemon` episodic memory modules, `cargo test`

---

## File Structure

- Modify: `crates/amux-daemon/src/agent/episodic/mod.rs`
  - Add `ConstraintState` and extend `NegativeConstraint` with state/provenance fields.
- Modify: `crates/amux-daemon/src/agent/episodic/schema.rs`
  - Add additive SQLite migration columns for richer negative knowledge state.
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
  - Add pure helpers for normalization, matching, state upgrade, propagation, row parsing, and prompt formatting.
  - Update `AgentEngine` methods for create/update/query/propagation behavior.
- Optionally modify: `crates/amux-daemon/src/agent/system_prompt.rs`
  - Only if wiring changes are needed beyond formatting content returned from `negative_knowledge.rs`.

### Task 1: Add Constraint State Model

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/mod.rs`
- Test: `crates/amux-daemon/src/agent/episodic/mod.rs`

- [ ] **Step 1: Write the failing serialization test for the new model**

Add a test near the existing `NegativeConstraint` serde test that builds a constraint with:
- `state = ConstraintState::Dead`
- `evidence_count = 3`
- `direct_observation = false`
- `derived_from_constraint_ids = vec!["nc-parent".into()]`
- `related_subject_tokens = vec!["deploy".into(), "config".into()]`

Assert JSON round-trip preserves all new fields.

Also add a backward-compat test that deserializes a pre-upgrade JSON payload without the new fields and asserts defaults are applied: `ConstraintState::Dying`, `1`, `true`, and empty vectors.

- [ ] **Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p tamux-daemon episodic::mod::tests::negative_constraint_round_trip_serialization`

Expected: FAIL because `ConstraintState` and/or the new fields do not exist yet.

- [ ] **Step 3: Implement the minimal model changes**

In `crates/amux-daemon/src/agent/episodic/mod.rs`:
- add `ConstraintState` with serde `snake_case` values: `Suspicious`, `Dying`, `Dead`
- extend `NegativeConstraint` with:

```rust
#[serde(default = "default_constraint_state")]
pub state: ConstraintState,
#[serde(default = "default_evidence_count")]
pub evidence_count: u32,
#[serde(default = "default_direct_observation")]
pub direct_observation: bool,
#[serde(default)]
pub derived_from_constraint_ids: Vec<String>,
#[serde(default)]
pub related_subject_tokens: Vec<String>,
```

- add helper defaults so deserializing older rows/json keeps working:

```rust
fn default_constraint_state() -> ConstraintState { ConstraintState::Dying }
fn default_evidence_count() -> u32 { 1 }
fn default_direct_observation() -> bool { true }
```

- derive `PartialEq, Eq` for `ConstraintState` and keep `NegativeConstraint` serde-compatible.

- [ ] **Step 4: Run the targeted test to verify it passes**

Run: `cargo test -p tamux-daemon episodic::mod::tests::negative_constraint_round_trip_serialization`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/mod.rs
git commit -m "feat: add constraint state metadata"
```

### Task 2: Add Schema Migration Coverage

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/schema.rs`
- Test: `crates/amux-daemon/src/agent/episodic/schema.rs`

- [ ] **Step 1: Write a failing schema migration test**

Add a test that creates an in-memory SQLite DB, runs `init_episodic_schema`, then inspects `PRAGMA table_info(negative_knowledge)` and asserts the new columns exist:
- `state`
- `evidence_count`
- `direct_observation`
- `derived_from_constraint_ids`
- `related_subject_tokens`

- [ ] **Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p tamux-daemon episodic::schema::tests::init_episodic_schema_adds_constraint_state_columns`

Expected: FAIL because the columns are not present.

- [ ] **Step 3: Implement the schema migration**

Update `crates/amux-daemon/src/agent/episodic/schema.rs`:
- add the new columns to the `CREATE TABLE IF NOT EXISTS negative_knowledge` statement
- add `ensure_column` calls for the five new columns so existing DBs migrate in place
- use defaults exactly as specified in the design:

```sql
state TEXT NOT NULL DEFAULT 'dying'
evidence_count INTEGER NOT NULL DEFAULT 1
direct_observation INTEGER NOT NULL DEFAULT 1
derived_from_constraint_ids TEXT NOT NULL DEFAULT '[]'
related_subject_tokens TEXT NOT NULL DEFAULT '[]'
```

- [ ] **Step 4: Run the targeted test to verify it passes**

Run: `cargo test -p tamux-daemon episodic::schema::tests::init_episodic_schema_adds_constraint_state_columns`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/schema.rs
git commit -m "feat: migrate negative knowledge state columns"
```

### Task 3: Add Pure Constraint-State Helpers

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- Test: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`

- [ ] **Step 1: Write failing tests for normalization, matching, and state upgrades**

Add unit tests for pure helpers that do not exist yet:
- `normalize_subject_tokens("Fix deploy-config in prod!") == vec!["config", "deploy", "fix", "prod"]`
- `normalized_subject_key(...)` returns a stable deduped string key
- `constraints_match_for_merge(...)` is true only for exact normalized subject + matching `solution_class` rules from the spec
- `related_for_propagation(...)` requires:
  - same `solution_class` and at least 2 shared tokens, or
  - without `solution_class`, at least 3 shared tokens
- `next_constraint_state(...)` upgrades `Suspicious -> Dying -> Dead` at the specified evidence thresholds

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::normalize_subject_tokens_` 

Expected: FAIL because the helper functions/tests are not implemented yet. If a name filter is too narrow, run the module: `cargo test -p tamux-daemon episodic::negative_knowledge::tests`.

- [ ] **Step 3: Implement the pure helpers**

In `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`, add pure functions such as:

```rust
pub fn normalize_subject_tokens(subject: &str) -> Vec<String>
pub fn normalized_subject_key(subject: &str) -> String
pub fn next_constraint_state(
    current: ConstraintState,
    evidence_count: u32,
    direct_observation: bool,
    confidence: f64,
) -> ConstraintState
pub fn constraints_match_for_merge(a: &NegativeConstraint, b: &NegativeConstraint) -> bool
pub fn related_for_propagation(source: &NegativeConstraint, target: &NegativeConstraint) -> bool
```

Implementation rules:
- lowercase
- split to alphanumeric tokens
- drop tokens shorter than 3 chars
- sort and dedupe
- exact normalized subject equality for merge
- monotonic state progression only

- [ ] **Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests`

Expected: PASS for the new helper tests.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: add constraint state helper rules"
```

### Task 4: Upgrade Prompt Formatting

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- Test: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`

- [ ] **Step 1: Write failing formatting tests**

Add tests that verify `format_negative_constraints`:
- groups/sorts strongest-first: `Dead`, then `Dying`, then `Suspicious`
- renders label text exactly:
  - `DO NOT attempt:` for `Dead`
  - `Avoid unless you have new evidence:` for `Dying`
  - `Use caution:` for `Suspicious`
- includes `State:`, `Reason:`, `Type:`, `confidence`, and `Source: direct|inferred`
- includes provenance text only when `derived_from_constraint_ids` is non-empty

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::format_negative_constraints_`

Expected: FAIL because formatting still uses the old flat output.

- [ ] **Step 3: Implement the formatting changes**

Update `format_negative_constraints` to:
- filter active constraints as before
- sort by state strength descending, then by `created_at` descending
- preserve the max-10 display cap
- render compact provenance only when available, for example:

```text
Source: inferred from 1 related dead constraint
```

Keep the overall section heading compatible with current prompt injection: `## Ruled-Out Approaches (Negative Knowledge)`.

- [ ] **Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::format_negative_constraints_`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: surface constraint states in prompts"
```

### Task 5: Add Persistence Round-Trip for Richer Constraints

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- Test: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`

- [ ] **Step 1: Write a failing row-mapping test**

Add a test that inserts a `negative_knowledge` row into an in-memory SQLite DB with all new columns populated, then reads it through the same row mapping logic and asserts:
- `state` parses correctly
- `evidence_count` is preserved
- `direct_observation` converts from integer to bool correctly
- JSON arrays in `derived_from_constraint_ids` and `related_subject_tokens` deserialize correctly

Add a second test for backward compatibility where those new columns are left at defaults and the parsed `NegativeConstraint` gets `Dying`, `1`, `true`, and empty vectors.

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::row_to_constraint_`

Expected: FAIL because row parsing does not read the new fields yet.

- [ ] **Step 3: Implement the row parsing and SQL updates**

Update `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`:
- add `constraint_state_to_str` / `str_to_constraint_state`
- update all `SELECT` lists to include the five new columns in a stable order
- update insert/upsert SQL in `add_negative_constraint`
- parse JSON arrays with safe defaults if empty
- preserve existing agent scoping behavior

- [ ] **Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::row_to_constraint_`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: persist richer negative knowledge constraints"
```

### Task 6: Implement Merge and Propagation Behavior

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- Test: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`

- [ ] **Step 1: Write failing behavior tests for merge and propagation**

Add tests that exercise pure or narrowly scoped helper logic for:
- creating a direct failure-derived constraint yields `Dying` by default
- high-confidence direct evidence yields `Dead`
- adding matching evidence to an existing `Suspicious` constraint upgrades it to `Dying`
- repeated matching evidence upgrades to `Dead` at count `>= 3`
- when a source becomes `Dead`, related targets are upgraded from `Suspicious` to `Dying`
- propagation appends the source id to `derived_from_constraint_ids`
- propagation does not overwrite `direct_observation = true`
- propagation sets `direct_observation = false` only for targets that have never had direct evidence
- propagation is capped at 10 targets and does not recurse

Prefer pure helper functions for the propagation transformation so tests stay unit-level and fast.

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::propagation_`

Expected: FAIL because merge and propagation logic is not implemented yet.

- [ ] **Step 3: Implement merge/update/propagation logic**

In `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`:
- add a helper to build a new direct constraint from an episode
- add a helper to merge evidence into an existing constraint
- update `add_negative_constraint` so it:
  - loads active constraints from cache or DB
  - finds merge candidates using `constraints_match_for_merge`
  - increments `evidence_count` and recomputes state instead of always inserting a new record
  - inserts a fresh row only when no merge candidate exists
- after a constraint reaches `Dead`, run one-hop propagation across related active constraints
- during propagation, append the source constraint id to `derived_from_constraint_ids`
- during propagation, set `direct_observation = false` only when the target has never had direct evidence
- sort propagation candidates deterministically before capping at 10, for example by state asc then `created_at` desc then `id`

If direct SQL updates become awkward, add a focused internal helper for persisting a fully materialized `NegativeConstraint` by id.

- [ ] **Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests`

Expected: PASS for merge and propagation tests.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: propagate related constraint states"
```

### Task 7: Wire Episode Recording to the New Model

**Files:**
- Modify: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`
- Test: `crates/amux-daemon/src/agent/episodic/negative_knowledge.rs`

- [ ] **Step 1: Write a failing test for failure-derived constraint initialization**

Add a test for the helper or integration path used by `record_negative_knowledge_from_episode` asserting:
- failure episode with root cause creates a constraint with computed normalized tokens
- `direct_observation = true`
- default state is `Dying`
- confidence `>= 0.85` upgrades initial state to `Dead`

- [ ] **Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::record_negative_knowledge_from_episode_`

Expected: FAIL because created constraints do not yet initialize the new fields correctly.

- [ ] **Step 3: Implement the initialization path**

Update `record_negative_knowledge_from_episode` to set:
- `state`
- `evidence_count = 1`
- `direct_observation = true`
- `derived_from_constraint_ids = vec![]`
- `related_subject_tokens = normalize_subject_tokens(&subject)`

Route creation through the upgraded add/merge path from Task 6.

- [ ] **Step 4: Run the targeted test to verify it passes**

Run: `cargo test -p tamux-daemon episodic::negative_knowledge::tests::record_negative_knowledge_from_episode_`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: initialize constraint state from episodes"
```

### Task 8: Run Verification Suite

**Files:**
- Modify: none expected

- [ ] **Step 1: Run focused module tests**

Run: `cargo test -p tamux-daemon episodic::`

Expected: PASS for all episodic module tests.

- [ ] **Step 2: Run the crate test suite if module tests pass**

Run: `cargo test -p tamux-daemon`

Expected: PASS, or if unrelated failures exist, capture them explicitly before proceeding.

- [ ] **Step 3: If formatting or borrow-checker cleanup was required, make the minimal fix**

Only address issues caused by this feature. Do not broaden scope.

- [ ] **Step 4: Re-run the affected tests**

Run the smallest command that proves the fix, then re-run `cargo test -p tamux-daemon episodic::`.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/episodic/mod.rs crates/amux-daemon/src/agent/episodic/schema.rs crates/amux-daemon/src/agent/episodic/negative_knowledge.rs
git commit -m "feat: upgrade negative knowledge constraint states"
```
