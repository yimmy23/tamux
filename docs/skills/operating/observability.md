# Observability — History Search, Code Symbols, Snapshots, and Git Status

Investigate past operations, navigate code, and manage workspace snapshots.

## Agent Rules

- **Search history before re-running work** — avoid duplicating commands already executed in this or prior sessions.
- **Use `find_symbol` for code navigation** — faster than grepping when you know the symbol name.
- **Use `list_snapshots` to understand rollback options** — snapshots are created automatically before managed commands.
- **Use `get_git_status` to understand repo state** — check before making git operations.
- **Use `search_history` with specific queries** — broad queries return too many results.

## Reference

### Tool: `search_history`

**Description:** Full-text search across the daemon's command and transcript history using SQLite FTS5.

| Param | Type | Required | Description |
|---|---|---|---|
| `query` | string | Yes | Search query (supports FTS5 syntax: AND, OR, NOT, "phrases", prefix*) |
| `limit` | integer | No | Max results to return |

**Returns:**

```json
{
  "query": "cargo test",
  "summary": "Found 15 matches",
  "hits": [
    {
      "timestamp": "2024-03-15T10:30:00Z",
      "session_id": "...",
      "content": "$ cargo test --workspace\ntest result: ok. 42 passed; 0 failed",
      "score": 0.95
    }
  ]
}
```

**FTS5 query examples:**

- `"cargo test"` — exact phrase match
- `cargo AND test AND fail*` — all terms, with prefix matching
- `NOT deploy` — exclude results containing "deploy"

### Tool: `find_symbol`

**Description:** Search for code symbols (functions, structs, classes, variables) using tree-sitter AST indexing.

| Param | Type | Required | Description |
|---|---|---|---|
| `workspace_root` | string | Yes | Absolute path to the project root |
| `symbol` | string | Yes | Symbol name to search for |
| `limit` | integer | No | Max results |

**Returns:** Array of symbol matches with file path, line number, symbol kind (function, struct, etc.), and context.

### Tool: `list_snapshots`

**Description:** List recorded workspace filesystem snapshots. Snapshots are auto-created before managed command execution.

| Param | Type | Required | Description |
|---|---|---|---|
| `workspace_id` | string | No | Filter by workspace (omit for all) |

**Returns:** Array of snapshot objects with: id, workspace_id, timestamp, description, backend (tar/zfs/btrfs).

### Tool: `restore_snapshot`

**Description:** Restore a previously recorded filesystem snapshot.

| Param | Type | Required | Description |
|---|---|---|---|
| `snapshot_id` | string | Yes | Snapshot UUID to restore |

**Returns:** `{ snapshot_id, ok: true/false, message: "..." }`

**Warning:** Restoring a snapshot reverts filesystem changes. Uncommitted work will be lost.

### Tool: `get_git_status`

**Description:** Get git status for a working directory.

| Param | Type | Required | Description |
|---|---|---|---|
| `path` | string | Yes | Absolute path to the git repository |

**Returns:**

```json
{
  "branch": "main",
  "is_dirty": true,
  "ahead": 2,
  "behind": 0,
  "untracked": ["new-file.rs"],
  "modified": ["src/main.rs"],
  "staged": ["Cargo.toml"]
}
```

### Tool: `verify_integrity`

**Description:** Verify WORM telemetry ledger integrity (SHA-256 hash chain validation).

**Parameters:** None

**Returns:** Verification results for all ledgers.

### Typical Investigation Workflow

```
1. search_history("error deploy") → find when the error started
2. get_git_status(path="/home/user/project") → check repo state
3. find_symbol(workspace_root="/home/user/project", symbol="deploy_handler") → locate code
4. list_snapshots() → find pre-error snapshot
5. restore_snapshot(snapshot_id="...") → roll back if needed
```

## Gotchas

- `search_history` uses FTS5 syntax — special characters may need quoting.
- `find_symbol` requires tree-sitter indexing to be built — first search may be slow.
- Snapshots have storage costs — old snapshots may be pruned automatically.
- `restore_snapshot` is destructive — it replaces current filesystem state.
- `get_git_status` requires the path to be a valid git repository.
- History search covers daemon-managed operations — commands run outside tamux will not appear.
