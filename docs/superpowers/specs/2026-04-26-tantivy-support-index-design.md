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
