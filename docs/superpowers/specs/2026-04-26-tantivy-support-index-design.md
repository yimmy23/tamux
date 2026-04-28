# Tantivy Support Search Index Design

## Summary

Use Tantivy as a supporting full-text search index for daemon-owned searchable data while keeping SQLite as the canonical database. SQLite remains responsible for durable records, migrations, transactions, relational filtering, and hydration. Tantivy stores a rebuildable projection of searchable text and metadata so history, memory, episodes, commands, skills, and agent persona records can be searched with ranked full-text retrieval.

Tantivy is a Lucene-style Rust search library with BM25 scoring, phrase queries, configurable tokenizers, incremental indexing, fast startup, snippets, and stored fields. It is not a relational database and does not provide embedding-based semantic search on its own. For this project, "semantic search" should mean ranked text retrieval plus structured metadata and graph signals. If true vector semantic search is required, Tantivy can be combined with the existing LanceDB-backed skill mesh later.

## Goals

- Add a daemon-local Tantivy index under the Tamux data root.
- Route database-backed full-text searches through Tantivy where the current implementation uses SQLite FTS5 or ad hoc `LIKE`.
- Keep SQLite as the source of truth and hydrate search results from SQLite records.
- Index all built-in and configured agent persona records, including the Slavic "god" agents such as `svarog`, `weles`, `rarog`, and any configured subagents.
- Support cross-source search over history entries, command logs, agent threads/messages, context archives, episodic memories, guidelines, generated skills, skill variants, memory graph nodes, and persona metadata.
- Make the index safely rebuildable from SQLite.

## Non-Goals

- Do not replace SQLite persistence.
- Do not move relational list/detail APIs to Tantivy.
- Do not implement vector embeddings in this phase.
- Do not introduce a standalone search server.
- Do not remove LanceDB skill mesh functionality.

## Current Search Surface

The daemon currently has several SQLite-backed search paths:

- `HistoryStore::search` queries `history_fts` in `crates/amux-daemon/src/history/core.rs`.
- `HistoryStore::search_context_archive` queries `context_archive_fts`, then falls back to `LIKE`, in `crates/amux-daemon/src/history/context_archive.rs`.
- Episodic retrieval queries `episodes_fts` in `crates/amux-daemon/src/agent/episodic/retrieval.rs`.
- Skill generation and recommendation use selected `LIKE` searches over skill metadata and memory nodes.
- Guideline discovery is exposed through `discover_guidelines`, `list_guidelines`, and `read_guideline`; guidelines live under the canonical runtime guidelines directory and sit above skills in the agent workflow.
- The command log currently supports filtering and listing but not ranked text search.

These paths should converge on a daemon search facade instead of each module owning its own text-search behavior.

## Architecture

Create a focused `history::search_index` module that owns Tantivy schema, indexing, query parsing, result ranking, and rebuilds. The rest of the daemon talks to a narrow `SearchIndex` interface through `HistoryStore`.

SQLite write paths emit search-index upserts after the canonical SQLite write succeeds. Search-index failures should be logged and surfaced only where the caller explicitly requested search. Normal persistence must not fail because Tantivy is temporarily unavailable.

Search requests should:

1. Parse the user query with a Tantivy `QueryParser`.
2. Apply source and metadata filters where available.
3. Retrieve ranked `SearchHitRef` values from Tantivy.
4. Hydrate full rows from SQLite using `source_kind` and `source_id`.
5. Fall back to existing SQLite search for the first implementation phase if the Tantivy index is missing, corrupt, or rebuilding.

## Tantivy Document Schema

Each indexed document should use common fields:

- `source_kind`: string, fast/stored. Examples: `history_entry`, `command_log`, `agent_message`, `context_archive`, `episode`, `guideline`, `generated_skill`, `skill_variant`, `memory_node`, `persona`.
- `source_id`: string, fast/stored. Primary key in the source table or generated persona id.
- `workspace_id`: optional string, fast/stored.
- `thread_id`: optional string, fast/stored.
- `agent_id`: optional string, fast/stored.
- `title`: text/stored.
- `body`: text/stored.
- `tags`: text/stored.
- `timestamp`: i64 fast/stored.
- `updated_at`: i64 fast/stored.
- `metadata_json`: stored text for lightweight display and debugging.

The unique Tantivy term should be `source_kind + ":" + source_id`. Updates delete by that term, then add the new document.

## Indexed Sources

Index these sources in the first complete pass:

- `history_entries`: title, excerpt, content, kind, path.
- `command_log`: command, cwd, path, workspace, pane, exit code metadata.
- `agent_threads`: title, last preview, agent name, metadata.
- `agent_messages`: role, content, provider/model, reasoning, tool call names.
- `context_archive`: summary, compressed content, original role, metadata.
- `episodes`: goal text, summary, outcome, root cause, entities, causal chain, solution class.
- `guidelines`: frontmatter name, title, description, recommended skills, relative path, and Markdown body.
- `generated_tools` and skill-generation records: name, description, schema, tags, status.
- `skill_variants`: skill name, variant name, context tags, lifecycle status, evidence summaries.
- `memory_nodes`: label, summary text, node type, confidence and access metadata.
- Persona records: built-in handles and configured subagents, including role id, display name, system prompt snippets, descriptions, skills, and governance notes.

## Guideline Indexing

Guidelines are a first-class knowledge surface above skills. They are local Markdown playbooks installed under the canonical runtime guidelines directory, and agents are expected to discover and read guidelines before selecting detailed skill workflows.

The Tantivy index should include guidelines as `source_kind = "guideline"` documents. Guideline documents should be built from files collected through the existing guideline discovery path, including bundled defaults copied into the runtime directory and user-created Markdown files.

Guideline documents should index:

- frontmatter `name`, `title`, `description`, and `recommended_skills`;
- relative path and file stem for lookup compatibility with `read_guideline`;
- Markdown headings and body content;
- tags derived from recommended skills and frontmatter fields;
- source metadata that preserves whether the guideline came from a bundled default or user file when that is available.

Guideline search results should recommend `read_guideline <name-or-path>` rather than `read_skill`. Existing `discover_guidelines`, `list_guidelines`, and `read_guideline` protocol behavior should remain intact, but guideline ranking can use Tantivy once parity tests prove it returns the same or better candidates than the current daemon-backed discovery pipeline.

## Expanded Capability Index Candidates

The index should eventually cover every text-bearing surface that can improve agent recall, self-correction, planning, and operator support. These sources should be added in priority order after the initial history/guideline/persona work.

Priority 1: capability multipliers.

- `causal_traces`: selected and rejected options, causal factors, outcomes, decision type, trace family, model. This is the main "why did we do that?" and "what alternatives failed?" corpus.
- `action_audit`: action type, summary, explanation, confidence band, linked causal trace, raw audit data. This makes provenance and past rationale searchable from agent tools.
- Failed execution records: `agent_tasks.error`, `agent_tasks.last_error`, `agent_tasks.result`, `agent_task_logs.message/details`, `goal_run_steps.error`, `goal_run_events.message/details`, `execution_traces.outcome/tool_sequence_json/metrics_json`, `subagent_metrics.health_state`. This is the corpus for "find similar failed tool calls" and "avoid repeating that failure".
- `memory_provenance`, `memory_provenance_relationships`, and `memory_tombstones`: accepted, retracted, replaced, and related facts. This lets agents search memory history without treating stale facts as current truth.
- `counterfactual_evaluations` and `dream_cycles`: counterfactual descriptions, source tasks, variation type, estimated savings, score, threshold status. This is high-value planning material because it records what Tamux believes would have worked better.
- Metacognition and adaptation: `cognitive_biases`, `workflow_profiles`, `implicit_signals`, `satisfaction_scores`, `cognitive_resonance_samples`, `behavior_adjustments_log`, `intent_predictions`, `system_outcome_predictions`, `temporal_patterns`, `temporal_predictions`, and `precomputation_log`. These rows should index human-readable trigger patterns, mitigation prompts, workflow names, predicted/actual actions, outcome descriptions, temporal pattern descriptions, and behavior adjustment reasons.

Priority 2: collaborative reasoning and learning.

- Debate and critique records: `debate_sessions`, `debate_arguments`, `debate_verdicts`, `critique_sessions`, `critique_arguments`, `critique_resolutions`. Index claims, evidence, verdicts, resolutions, risk scores, and confidence so agents can reuse previous deliberation.
- Collaboration and consensus records: `collaboration_sessions`, `collaboration_agent_outcomes`, `consensus_bids`, `role_assignments`, and `consensus_quality_metrics`. Index agent reasoning, role assignments, outcomes, prediction errors, and learned scores.
- Skill evolution: `skill_variants`, `skill_variant_usage`, `skill_variant_history`, `gene_pool`, `gene_fitness_history`, and `gene_crossbreeds`. Index skill names, variant names, context tags, lifecycle status, outcomes, and parent/offspring relationships.
- Morphogenesis and soul adaptation: `morphogenesis_affinities`, `affinity_updates_log`, and `soul_adaptations_log`. Index domains, trigger types, adaptation type, and soul snippets so role specialization history is searchable.
- Emergent protocol learning: `thread_protocol_candidates`, `emergent_protocols`, `protocol_steps`, and `protocol_usage_log`. Index protocol descriptions, normalized patterns, step intents, tool names, success/fallback reasons, and source thread.

Priority 3: operational continuity.

- Goal and task surfaces: `goal_runs`, `goal_run_steps`, `goal_run_events`, `agent_tasks`, `agent_task_logs`, `workspace_tasks`, and `workspace_notices`. Index titles, goals, instructions, success criteria, summaries, notices, blocked reasons, compensation summaries, and failure causes.
- Context and memory surfaces: `context_archive`, `thread_structural_memory`, `memory_graph_clusters`, `memory_cluster_members`, `memory_nodes`, `offloaded_payloads`, and `memory_distillation_log`. Index summaries, compressed content, graph labels/summaries, offloaded payload summaries, distilled facts, categories, and target memory files.
- Recovery and liveness: `agent_checkpoints`, `agent_health_log`, `heartbeat_history`, `harness_state_records`, and `forge_pass_log`. Index checkpoint summaries, health interventions, heartbeat digests, harness summaries, and forge pass pattern counts.
- Operator and profile state: `operator_profile_fields`, `operator_profile_events`, `operator_profile_sessions`, `operator_profile_checkins`, and non-sensitive concierge/profile summaries. Index only consented, non-secret profile content.
- External workflow surfaces: `plugins`, `event_triggers`, `routine_definitions`, `gateway_health_snapshots`, `external_runtime_profiles`, and `browser_profiles`. Index names, descriptions, manifest summaries, trigger templates, routine descriptions, profile labels, and health summaries.

Do not index secrets or authentication material. Exclude `provider_auth_state`, `plugin_credentials`, secret `plugin_settings`, raw auth JSON, encrypted blobs, and any field marked secret. For large JSON fields, extract short searchable summaries and known text fields rather than dumping entire payloads into Tantivy.

## Public API Shape

Add daemon-internal types:

```rust
pub(crate) struct SearchIndex;

pub(crate) struct SearchRequest {
    pub query: String,
    pub limit: usize,
    pub source_kinds: Vec<SearchSourceKind>,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
}

pub(crate) struct SearchHitRef {
    pub source_kind: SearchSourceKind,
    pub source_id: String,
    pub score: f32,
    pub title: String,
    pub snippet: Option<String>,
    pub timestamp: Option<i64>,
}
```

`HistoryStore` should expose:

- `search_index_upsert_*` helpers for source write paths.
- `search_index_delete(source_kind, source_id)` for deletes.
- `search_index_rebuild()` for startup repair and explicit maintenance.
- `search_index_query(request)` for ranked search.

Existing public protocol messages should remain stable in the first phase. New capabilities can be added after the index is reliable.

## Data Flow

On daemon startup:

1. Open SQLite.
2. Open or create Tantivy index under `<data-root>/search-index/tantivy`.
3. Check the index metadata version.
4. If missing, incompatible, or explicitly marked stale, rebuild from SQLite.

On writes:

1. Commit the SQLite transaction.
2. Build one or more Tantivy documents from the committed row.
3. Delete the old Tantivy document by unique key.
4. Add the new document.
5. Commit on a debounce or batch boundary to avoid excessive small commits.

On search:

1. Query Tantivy for ranked hits.
2. Hydrate rows from SQLite.
3. Drop hits that no longer exist in SQLite.
4. Return existing response shapes where possible.

## Error Handling

- If Tantivy open fails, keep the daemon running and mark search index unavailable.
- If indexing a row fails, log source kind and source id, mark index stale, and continue the SQLite write.
- If query parsing fails, retry with escaped tokenized terms.
- If the index schema version changes, rebuild.
- If search is unavailable, use the existing SQLite FTS5 or `LIKE` path during migration.

## Testing

Use TDD for implementation:

- Unit-test query parsing and fallback escaping.
- Unit-test document builders for each source kind.
- Integration-test rebuilding from an in-memory or temporary SQLite store.
- Integration-test upsert, delete, and reindex behavior.
- Regression-test current `search_history`, `search_context_archive`, and episodic retrieval results.
- Add guideline indexing tests that prove installed Markdown guidelines can be ranked and still resolve through `read_guideline`.
- Add persona indexing tests that prove built-in and configured agent personas are searchable.

## Rollout

1. Add Tantivy dependency and a hidden search-index module with tests.
2. Implement indexing for `history_entries` and adapt `HistoryStore::search`.
3. Add context archive and episodic retrieval support.
4. Add command log, agent message, guideline, skill, memory node, and persona indexing.
5. Add rebuild and stale-index repair.
6. Remove obsolete SQLite FTS5 paths only after parity tests pass.

## Open Questions

- Whether to keep SQLite FTS5 tables indefinitely as fallback or remove them after a migration period.
- Whether command-log text search needs a new protocol/API parameter or should only feed global search.
- Whether "semantic search" should later include embeddings by fusing Tantivy results with LanceDB vector results.
