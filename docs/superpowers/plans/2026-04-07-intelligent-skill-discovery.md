# Intelligent Skill Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a daemon-enforced skill discovery system that ranks installed skills before non-trivial work, requires agent compliance or explicit skip rationale, exposes the same structured recommendation result in CLI and MCP, and adds operator-facing settings plus a non-blocking community scout path.

**Architecture:** Introduce a shared daemon skill-recommendation service and typed public result schema, then route agent preflight, CLI, MCP, TUI, and Electron through that shared layer. Enforce discovery in the daemon send loop, persist the latest discovery/compliance state on threads, and use workflow notices plus settings-backed UX to surface the recommendation and background community scout behavior.

**Tech Stack:** Rust workspace (`amux-daemon`, `amux-protocol`, `amux-cli`, `amux-mcp`, `amux-tui`), serde/JSON, existing SQLite-backed history store, React + TypeScript + Electron, ratatui-based TUI.

---

## File Structure

### New files

- `crates/amux-daemon/src/agent/skill_recommendation/mod.rs`
  Responsibility: module entrypoint and orchestration helpers for recommendation flow.
- `crates/amux-daemon/src/agent/skill_recommendation/types.rs`
  Responsibility: internal recommendation/compliance types and tier/action enums.
- `crates/amux-daemon/src/agent/skill_recommendation/metadata.rs`
  Responsibility: parse `SKILL.md` content into searchable metadata fields.
- `crates/amux-daemon/src/agent/skill_recommendation/ranking.rs`
  Responsibility: candidate scoring, thresholds, and diversity selection.
- `crates/amux-daemon/src/agent/tests/skill_recommendation.rs`
  Responsibility: unit tests for metadata extraction, ranking, thresholds, and skip-policy decisions.

### Existing files to modify

- `crates/amux-protocol/src/messages/support.rs`
  Add public discovery result/candidate structs.
- `crates/amux-protocol/src/messages/client.rs`
  Add `ClientMessage::SkillDiscover`.
- `crates/amux-protocol/src/messages/daemon.rs`
  Add `DaemonMessage::SkillDiscoverResult`.
- `crates/amux-protocol/src/messages/tests/mod.rs`
  Add protocol round-trip coverage for new messages and payload structs.
- `crates/amux-daemon/src/agent/mod.rs`
  Register the new recommendation module.
- `crates/amux-daemon/src/agent/skill_preflight.rs`
  Reuse the shared ranking service instead of maintaining separate heuristics.
- `crates/amux-daemon/src/agent/system_prompt.rs`
  Replace `list_skills` guidance with hard-gate `discover_skills` guidance.
- `crates/amux-daemon/src/agent/prompt_inspection.rs`
  Keep prompt inspection output aligned with the new discovery workflow.
- `crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs`
  Run discovery before request construction and attach discovery context.
- `crates/amux-daemon/src/agent/agent_loop/send_message/tool_calls.rs`
  Enforce compliance before substantial tool execution.
- `crates/amux-daemon/src/agent/types/thread_message_types.rs`
  Persist latest discovery/compliance state on the thread snapshot.
- `crates/amux-daemon/src/agent/metadata.rs`
  Parse/build thread metadata for discovery/compliance state.
- `crates/amux-daemon/src/agent/tests/messaging/part1.rs`
  Cover persisted thread metadata for the latest discovery state.
- `crates/amux-daemon/src/agent/agent_loop/tests/part2.rs`
  Cover hard-gate enforcement and skip-rationale behavior.
- `crates/amux-daemon/src/server/dispatch_part7.rs`
  Handle `ClientMessage::SkillDiscover`.
- `crates/amux-daemon/src/server/tests_part3.rs`
  Add daemon dispatch coverage for discovery requests.
- `crates/amux-daemon/src/agent/types/config_skill.rs`
  Add a new recommendation config struct. Use `skill_recommendation`, not `skill_discovery`, to avoid colliding with the existing trace-to-skill-generation config.
- `crates/amux-daemon/src/agent/types/config_core.rs`
  Add the `skill_recommendation` config field to `AgentConfig`.
- `crates/amux-daemon/src/agent/types/runtime_config.rs`
  Wire defaults for `skill_recommendation`.
- `crates/amux-cli/src/cli.rs`
  Add `tamux skill discover`.
- `crates/amux-cli/src/client/skill_api.rs`
  Add `send_skill_discover`.
- `crates/amux-cli/src/commands/skills.rs`
  Render ranked discovery output and next-action guidance.
- `crates/amux-mcp/src/main.rs`
  Add `discover_skills` tool handler using daemon round-trips rather than local plain-file listing.
- `crates/amux-mcp/src/main/tool_definitions.rs`
  Register `discover_skills` and keep `list_skills` as the raw catalog view.
- `crates/amux-tui/src/widgets/settings/part8.rs`
  Render new recommendation/community-scout settings.
- `crates/amux-tui/src/widgets/settings/part7.rs`
  Update field indexes for the new settings rows.
- `crates/amux-tui/src/state/settings.rs`
  Add field IDs for the new settings entries.
- `crates/amux-tui/src/app/settings_handlers/impl_part5.rs`
  Toggle boolean recommendation settings.
- `crates/amux-tui/src/app/settings_handlers/impl_part6.rs`
  Edit numeric recommendation settings.
- `crates/amux-tui/src/app/modal_handlers.rs`
  Save recommendation settings back to daemon config paths.
- `crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs`
  Add TUI settings handler coverage.
- `crates/amux-tui/src/app/events/events_activity.rs`
  Surface skill gate/compliance workflow notices more explicitly in activity/status.
- `frontend/src/lib/agentStore/settings.ts`
  Add `skill_recommendation` settings to the React store and defaults.
- `frontend/src/lib/agentDaemonConfig.ts`
  Serialize new settings into daemon config payloads.
- `frontend/src/lib/agentDaemonConfig.spec.ts`
  Cover daemon config serialization for recommendation settings.
- `frontend/src/lib/agentStore/settings.spec.ts`
  Cover normalization of recommendation settings from daemon state.
- `frontend/src/components/settings-panel/AgentTab.tsx`
  Add React settings controls for local hard gate and community-scout behavior.
- `frontend/src/components/agent-chat-panel/runtime/daemonHelpers.ts`
  Parse discovery/compliance workflow notice details into operator-facing operational events.
- `frontend/src/lib/agent-mission-store/types.ts`
  Extend operational event kinds for discovery/compliance/community-scout notices.

### Scope note

The repo already uses `skill_discovery` for trace complexity and skill generation thresholds. Do not overload that field for the new runtime recommender. Introduce a separate `skill_recommendation` config namespace, then add TUI/frontend migration logic so old `/skill_discovery/enabled` UI state is no longer the source of truth.

### Task 1: Define Protocol And Config Shapes

**Files:**
- Modify: `crates/amux-protocol/src/messages/support.rs`
- Modify: `crates/amux-protocol/src/messages/client.rs`
- Modify: `crates/amux-protocol/src/messages/daemon.rs`
- Modify: `crates/amux-protocol/src/messages/tests/mod.rs`
- Modify: `crates/amux-daemon/src/agent/types/config_skill.rs`
- Modify: `crates/amux-daemon/src/agent/types/config_core.rs`
- Modify: `crates/amux-daemon/src/agent/types/runtime_config.rs`

- [ ] **Step 1: Write the failing protocol/config tests**

Add tests for:

```rust
#[test]
fn skill_discover_result_round_trip() {
    let msg = DaemonMessage::SkillDiscoverResult {
        result_json: serde_json::json!({
            "query": "debug panic",
            "confidence_tier": "strong",
            "recommended_action": "read_skill",
            "candidates": [{
                "skill_name": "systematic-debugging",
                "score": 93.0,
                "reasons": ["matched debug", "workspace rust", "active variant"]
            }]
        }).to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    matches!(decoded, DaemonMessage::SkillDiscoverResult { .. });
}

#[test]
fn skill_recommendation_config_defaults() {
    let cfg = AgentConfig::default();
    assert!(cfg.skill_recommendation.enabled);
    assert!(cfg.skill_recommendation.require_read_on_strong_match);
}
```

- [ ] **Step 2: Run protocol/config tests to verify they fail**

Run: `cargo test -p amux-protocol skill_discover_result_round_trip -- --exact`
Expected: FAIL because `SkillDiscoverResult` does not exist yet.

Run: `cargo test -p amux-daemon skill_recommendation_config_defaults -- --exact`
Expected: FAIL because `skill_recommendation` config does not exist yet.

- [ ] **Step 3: Add typed public payloads and messages**

Implement public structs in `support.rs` similar to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryCandidatePublic {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub relative_path: String,
    pub status: String,
    pub score: f64,
    pub confidence_tier: String,
    pub reasons: Vec<String>,
    pub context_tags: Vec<String>,
    pub use_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryResultPublic {
    pub query: String,
    pub required: bool,
    pub confidence_tier: String,
    pub recommended_action: String,
    pub explicit_rationale_required: bool,
    pub workspace_tags: Vec<String>,
    pub candidates: Vec<SkillDiscoveryCandidatePublic>,
}
```

Add:

```rust
ClientMessage::SkillDiscover {
    query: String,
    session_id: Option<SessionId>,
    limit: usize,
}

DaemonMessage::SkillDiscoverResult {
    result_json: String,
}
```

Add the new config namespace:

```rust
pub struct SkillRecommendationConfig {
    pub enabled: bool,
    pub require_read_on_strong_match: bool,
    pub strong_match_threshold: f64,
    pub weak_match_threshold: f64,
    pub background_community_search: bool,
    pub community_preapprove_timeout_secs: u64,
    pub suggest_global_enable_after_approvals: u32,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p amux-protocol skill_discover_result_round_trip -- --exact`
Expected: PASS

Run: `cargo test -p amux-daemon skill_recommendation_config_defaults -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/amux-protocol/src/messages/support.rs \
        crates/amux-protocol/src/messages/client.rs \
        crates/amux-protocol/src/messages/daemon.rs \
        crates/amux-protocol/src/messages/tests/mod.rs \
        crates/amux-daemon/src/agent/types/config_skill.rs \
        crates/amux-daemon/src/agent/types/config_core.rs \
        crates/amux-daemon/src/agent/types/runtime_config.rs
git commit -m "feat: add skill discovery protocol and config types"
```

### Task 2: Build The Shared Daemon Recommendation Engine

**Files:**
- Create: `crates/amux-daemon/src/agent/skill_recommendation/mod.rs`
- Create: `crates/amux-daemon/src/agent/skill_recommendation/types.rs`
- Create: `crates/amux-daemon/src/agent/skill_recommendation/metadata.rs`
- Create: `crates/amux-daemon/src/agent/skill_recommendation/ranking.rs`
- Modify: `crates/amux-daemon/src/agent/mod.rs`
- Modify: `crates/amux-daemon/src/agent/skill_preflight.rs`
- Test: `crates/amux-daemon/src/agent/tests/skill_recommendation.rs`

- [ ] **Step 1: Write the failing daemon ranking tests**

Add tests for:

```rust
#[test]
fn extract_skill_metadata_reads_description_and_triggers() {
    let md = r#"---
name: systematic-debugging
description: Use when fixing bugs
---

## When to Use
- Debugging
- Test failures
"#;
    let meta = extract_skill_metadata(md);
    assert!(meta.search_text.contains("fixing bugs"));
    assert!(meta.keywords.iter().any(|k| k == "debugging"));
}

#[tokio::test]
async fn rank_skill_candidates_prefers_context_and_success() {
    // register two skills, one matching request+workspace with better success
    // assert top result is the matching reliable skill
}

#[test]
fn confidence_tier_is_none_when_scores_do_not_clear_threshold() {
    assert_eq!(confidence_tier(7.0, None, &cfg), ConfidenceTier::None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p amux-daemon extract_skill_metadata_reads_description_and_triggers -- --exact`
Expected: FAIL because the shared recommendation module does not exist yet.

- [ ] **Step 3: Implement metadata extraction and ranking**

Implement a shared service with APIs shaped like:

```rust
pub async fn discover_local_skills(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult>;
```

Core scoring formula:

```rust
let score =
    lexical_overlap * 28.0 +
    workspace_overlap * 12.0 +
    success_rate_bonus +
    use_count_bonus +
    recency_bonus +
    lifecycle_bonus +
    built_in_bonus;
```

Require:

- metadata parsing from `SKILL.md` frontmatter and headings,
- family de-duplication by `skill_name`,
- `strong` / `weak` / `none` confidence tier selection,
- explicit `recommended_action` values (`read_skill`, `justify_skip`, `none`),
- reuse from `skill_preflight.rs` so there is one ranking implementation.

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -p amux-daemon skill_recommendation -- --nocapture`
Expected: PASS for metadata extraction, ranking, and threshold tests.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/skill_recommendation \
        crates/amux-daemon/src/agent/mod.rs \
        crates/amux-daemon/src/agent/skill_preflight.rs \
        crates/amux-daemon/src/agent/tests/skill_recommendation.rs
git commit -m "feat: add shared daemon skill recommendation engine"
```

### Task 3: Expose Discovery Through Daemon Dispatch, CLI, And MCP

**Files:**
- Modify: `crates/amux-daemon/src/server/dispatch_part7.rs`
- Modify: `crates/amux-daemon/src/server/tests_part3.rs`
- Modify: `crates/amux-cli/src/cli.rs`
- Modify: `crates/amux-cli/src/client/skill_api.rs`
- Modify: `crates/amux-cli/src/commands/skills.rs`
- Modify: `crates/amux-mcp/src/main.rs`
- Modify: `crates/amux-mcp/src/main/tool_definitions.rs`

- [ ] **Step 1: Write failing dispatch and CLI tests**

Add daemon dispatch coverage like:

```rust
#[tokio::test]
async fn skill_discover_returns_ranked_candidates() {
    // send ClientMessage::SkillDiscover and assert result_json contains candidates
}
```

Add CLI parsing coverage for:

```rust
tamux skill discover "debug panic"
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p amux-daemon skill_discover_returns_ranked_candidates -- --exact`
Expected: FAIL because dispatch does not handle `SkillDiscover`.

- [ ] **Step 3: Implement daemon, CLI, and MCP surfaces**

Implement:

```rust
ClientMessage::SkillDiscover { query, session_id, limit }
```

Dispatch result with:

```rust
DaemonMessage::SkillDiscoverResult {
    result_json: serde_json::to_string(&public_result)?,
}
```

Add CLI command:

```rust
SkillAction::Discover { query: String, limit: usize }
```

Render output similar to:

```text
Confidence: strong
Next action: read_skill systematic-debugging

1. systematic-debugging [active] score=93
   reasons: matched debug, workspace rust, 14/16 successful uses
```

Add MCP tool:

```json
{
  "name": "discover_skills",
  "description": "Rank installed tamux skills for a task and return the recommended next action."
}
```

Keep `list_skills` unchanged as the raw catalog view.

- [ ] **Step 4: Run focused tests**

Run: `cargo test -p amux-daemon skill_discover_returns_ranked_candidates -- --exact`
Expected: PASS

Run: `cargo test -p amux-cli skill -- --nocapture`
Expected: PASS for CLI skill command coverage.

Run: `cargo test -p amux-mcp tool_definitions -- --nocapture`
Expected: PASS and includes `discover_skills`.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/server/dispatch_part7.rs \
        crates/amux-daemon/src/server/tests_part3.rs \
        crates/amux-cli/src/cli.rs \
        crates/amux-cli/src/client/skill_api.rs \
        crates/amux-cli/src/commands/skills.rs \
        crates/amux-mcp/src/main.rs \
        crates/amux-mcp/src/main/tool_definitions.rs
git commit -m "feat: expose skill discovery via daemon cli and mcp"
```

### Task 4: Enforce The Hard Gate In The Agent Runtime

**Files:**
- Modify: `crates/amux-daemon/src/agent/system_prompt.rs`
- Modify: `crates/amux-daemon/src/agent/prompt_inspection.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/send_message/tool_calls.rs`
- Modify: `crates/amux-daemon/src/agent/types/thread_message_types.rs`
- Modify: `crates/amux-daemon/src/agent/metadata.rs`
- Modify: `crates/amux-daemon/src/agent/tests/messaging/part1.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/tests/part2.rs`

- [ ] **Step 1: Write the failing runtime tests**

Add tests for:

```rust
#[tokio::test]
async fn strong_match_requires_read_skill_before_non_discovery_tool() {
    // assistant emits execute_command without read_skill after strong discover result
    // assert runtime injects error/tool result and blocks execution
}

#[tokio::test]
async fn weak_match_allows_progress_only_after_skip_rationale() {
    // assistant emits explicit rationale tool/message, then proceeds
    // assert runtime allows next tool call
}

#[test]
fn thread_metadata_round_trips_latest_skill_discovery_state() {
    // persist and reload thread.latest_skill_discovery
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p amux-daemon strong_match_requires_read_skill_before_non_discovery_tool -- --exact`
Expected: FAIL because the gate is not enforced yet.

- [ ] **Step 3: Add compliance state and tool-call enforcement**

Persist latest thread discovery state with fields like:

```rust
pub struct LatestSkillDiscoveryState {
    pub query: String,
    pub confidence_tier: String,
    pub recommended_skill: Option<String>,
    pub recommended_action: String,
    pub read_skill_identifier: Option<String>,
    pub skip_rationale: Option<String>,
    pub compliant: bool,
    pub updated_at: u64,
}
```

In `setup.rs`:

- run shared discovery for non-trivial user turns,
- attach structured discovery context to the request,
- emit a workflow notice describing confidence and next action.

In `tool_calls.rs`:

- allow `discover_skills`, `read_skill`, and the explicit skip-rationale path,
- block substantial tools when strong-match compliance is missing,
- emit a clear tool result and workflow notice when the gate stops execution.

Update prompt text and prompt inspection to say:

- discovery is mandatory before non-trivial work,
- `discover_skills` is the first tool,
- strong matches require `read_skill`,
- weak/none matches require explicit rationale before continuing.

- [ ] **Step 4: Run the targeted runtime tests**

Run: `cargo test -p amux-daemon strong_match_requires_read_skill_before_non_discovery_tool -- --exact`
Expected: PASS

Run: `cargo test -p amux-daemon weak_match_allows_progress_only_after_skip_rationale -- --exact`
Expected: PASS

Run: `cargo test -p amux-daemon thread_metadata_round_trips_latest_skill_discovery_state -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/system_prompt.rs \
        crates/amux-daemon/src/agent/prompt_inspection.rs \
        crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs \
        crates/amux-daemon/src/agent/agent_loop/send_message/tool_calls.rs \
        crates/amux-daemon/src/agent/types/thread_message_types.rs \
        crates/amux-daemon/src/agent/metadata.rs \
        crates/amux-daemon/src/agent/tests/messaging/part1.rs \
        crates/amux-daemon/src/agent/agent_loop/tests/part2.rs
git commit -m "feat: enforce skill discovery gate in agent runtime"
```

### Task 5: Add Operator Settings And Discovery UX In TUI And Electron

**Files:**
- Modify: `crates/amux-tui/src/widgets/settings/part8.rs`
- Modify: `crates/amux-tui/src/widgets/settings/part7.rs`
- Modify: `crates/amux-tui/src/state/settings.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part5.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part6.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs`
- Modify: `crates/amux-tui/src/app/events/events_activity.rs`
- Modify: `frontend/src/lib/agentStore/settings.ts`
- Modify: `frontend/src/lib/agentDaemonConfig.ts`
- Modify: `frontend/src/lib/agentDaemonConfig.spec.ts`
- Modify: `frontend/src/lib/agentStore/settings.spec.ts`
- Modify: `frontend/src/components/settings-panel/AgentTab.tsx`
- Modify: `frontend/src/components/agent-chat-panel/runtime/daemonHelpers.ts`
- Modify: `frontend/src/lib/agent-mission-store/types.ts`

- [ ] **Step 1: Write failing settings/UI tests**

Add TUI settings tests for new keys:

```rust
"/skill_recommendation/enabled"
"/skill_recommendation/background_community_search"
"/skill_recommendation/community_preapprove_timeout_secs"
```

Add frontend tests asserting:

```ts
expect(buildDaemonAgentConfig(settings).skill_recommendation.enabled).toBe(true);
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p amux-tui feat_skill_recommendation -- --nocapture`
Expected: FAIL because the new settings fields do not exist.

Run: `npm test -- agentDaemonConfig.spec.ts`
Expected: FAIL because `skill_recommendation` is not serialized yet.

- [ ] **Step 3: Implement settings and operator-visible notices**

In TUI and React add settings for:

- local hard gate enabled,
- background community search enabled,
- 30-second preapprove prompt timeout,
- suggestion threshold for enabling globally.

Serialize them under:

```json
{
  "skill_recommendation": {
    "enabled": true,
    "background_community_search": false,
    "community_preapprove_timeout_secs": 30,
    "suggest_global_enable_after_approvals": 3
  }
}
```

Update workflow notice parsing so kinds like:

- `skill-discovery-required`
- `skill-discovery-recommended`
- `skill-discovery-skipped`
- `skill-community-scout`

become first-class operational events rather than generic `skill-consulted`.

- [ ] **Step 4: Run focused UI tests**

Run: `cargo test -p amux-tui settings_handlers -- --nocapture`
Expected: PASS

Run: `cd frontend && npm test -- agentDaemonConfig.spec.ts settings.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/amux-tui/src/widgets/settings/part8.rs \
        crates/amux-tui/src/widgets/settings/part7.rs \
        crates/amux-tui/src/state/settings.rs \
        crates/amux-tui/src/app/settings_handlers/impl_part5.rs \
        crates/amux-tui/src/app/settings_handlers/impl_part6.rs \
        crates/amux-tui/src/app/modal_handlers.rs \
        crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs \
        crates/amux-tui/src/app/events/events_activity.rs \
        frontend/src/lib/agentStore/settings.ts \
        frontend/src/lib/agentDaemonConfig.ts \
        frontend/src/lib/agentDaemonConfig.spec.ts \
        frontend/src/lib/agentStore/settings.spec.ts \
        frontend/src/components/settings-panel/AgentTab.tsx \
        frontend/src/components/agent-chat-panel/runtime/daemonHelpers.ts \
        frontend/src/lib/agent-mission-store/types.ts
git commit -m "feat: add skill discovery settings and operator ux"
```

### Task 6: Add Background Community Scout And End-To-End Verification

**Files:**
- Modify: `crates/amux-daemon/src/agent/skill_recommendation/mod.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/tests/part2.rs`
- Modify: `crates/amux-cli/src/client/skill_api.rs`
- Modify: `crates/amux-cli/src/commands/skills.rs`
- Modify: `frontend/src/components/agent-chat-panel/runtime/daemonHelpers.ts`
- Modify: `frontend/src/lib/agent-mission-store/types.ts`

- [ ] **Step 1: Write the failing background-scout tests**

Add tests for:

```rust
#[tokio::test]
async fn local_strong_match_still_runs_when_background_community_scout_enabled() {
    // assert scout launches asynchronously but local recommendation remains authoritative
}

#[tokio::test]
async fn disabled_background_community_scout_does_not_search_registry() {
    // assert no registry call when feature flag is off
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p amux-daemon local_strong_match_still_runs_when_background_community_scout_enabled -- --exact`
Expected: FAIL because the scout path does not exist yet.

- [ ] **Step 3: Implement asynchronous scout behavior**

Implement best-effort async community search after local discovery:

- only when `skill_recommendation.background_community_search` is true,
- never blocks the current turn,
- records a `skill-community-scout` workflow notice,
- reuses existing community search plumbing rather than building a second registry client,
- packages suggested import candidates for UI consumption,
- leaves the current turn governed solely by installed-skill results.

The first cut can emit the candidate list and preapprove timeout through workflow notice `details` JSON instead of building a new approval transport.

- [ ] **Step 4: Run verification commands**

Run: `cargo test -p amux-daemon skill_recommendation -- --nocapture`
Expected: PASS

Run: `cargo test -p amux-cli skill -- --nocapture`
Expected: PASS

Run: `cargo test -p amux-mcp -- --nocapture`
Expected: PASS

Run: `cargo test -p amux-tui settings_handlers -- --nocapture`
Expected: PASS

Run: `cd frontend && npm test -- agentDaemonConfig.spec.ts settings.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/amux-daemon/src/agent/skill_recommendation/mod.rs \
        crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs \
        crates/amux-daemon/src/agent/agent_loop/tests/part2.rs \
        crates/amux-cli/src/client/skill_api.rs \
        crates/amux-cli/src/commands/skills.rs \
        frontend/src/components/agent-chat-panel/runtime/daemonHelpers.ts \
        frontend/src/lib/agent-mission-store/types.ts
git commit -m "feat: add background community skill scout"
```

## Verification Checklist

- `cargo test -p amux-protocol`
- `cargo test -p amux-daemon skill_recommendation -- --nocapture`
- `cargo test -p amux-daemon agent_loop -- --nocapture`
- `cargo test -p amux-cli skill -- --nocapture`
- `cargo test -p amux-mcp -- --nocapture`
- `cargo test -p amux-tui settings_handlers -- --nocapture`
- `cd frontend && npm test -- agentDaemonConfig.spec.ts settings.spec.ts`
- `cd frontend && npm run lint`

## Review Notes

- The plan intentionally separates the new runtime recommender config from the old `skill_discovery` generation thresholds to avoid schema confusion and broken settings semantics.
- The first community-scout implementation should prefer workflow-notice-driven UX over a brand-new approval transport, because that is the smallest route to non-blocking operator prompts without stalling the local hard gate.
