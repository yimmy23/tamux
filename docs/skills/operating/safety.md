# Approval Workflows, Risk Policies, and Sandbox Behavior

Understand how tamux enforces safety through policy evaluation, approval gates, sandboxing, and automatic snapshots.

## Agent Rules

- **Understand that tamux enforces safety by default** — commands matching risk patterns require operator approval
- **Always provide a clear `rationale`** when executing commands — it's shown to the operator during approval
- **Handle `approval_required` responses gracefully** — inform the user and wait, don't retry blindly
- **Don't attempt to bypass safety controls** — sandbox isolation and policy evaluation are non-negotiable
- **Know the risk patterns** — certain commands will always trigger approval (see list below)
- **Snapshots are automatic** — filesystem snapshots are taken before managed command execution

## Reference

### How the Safety System Works

tamux has a multi-layered safety system:

1. **Policy Engine** — Pattern-matches commands against known risk categories
2. **Sandbox Isolation** — Linux namespaces or macOS Seatbelt restrict filesystem/network access
3. **Approval Workflow** — High-risk commands pause for operator decision
4. **Filesystem Snapshots** — Automatic checkpoints before execution (rollback via `restore_snapshot`)
5. **WORM Telemetry** — Append-only integrity ledgers for audit trails
6. **Credential Scrubbing** — Automatic redaction of secrets in output

### Risk Patterns That Trigger Approval

The policy engine detects these categories:

| Category | Examples |
|---|---|
| Destructive filesystem | `rm -rf`, `shred`, `mkfs` |
| Git force operations | `git push --force`, `git reset --hard`, `git clean -fdx` |
| Infrastructure mutations | `terraform destroy`, `kubectl delete`, `docker rm` |
| Package/system modification | `apt install`, `pip install --system`, `npm install -g` |
| Pipe-to-shell | `curl ... \| bash`, `wget ... \| sh` |
| Network exfiltration | `curl -X POST` with data, `scp`, `rsync` to remote |
| Privilege escalation | `sudo`, `su`, `chmod 777` |
| Database mutations | `DROP TABLE`, `DELETE FROM`, `TRUNCATE` |

### Approval Flow

When a command triggers the policy engine:

1. `execute_command` returns `approval_required` event with:
   - `approval_id` — unique identifier
   - `command` — the command text
   - `rationale` — your provided reason
   - `risk_level` — low / medium / high / critical
   - `blast_radius` — scope of potential impact
   - `reasons` — specific policy rules that matched

2. The operator sees an approval modal with three options:
   - **Allow Once** — execute this command only
   - **Allow For Session** — auto-approve similar commands for this session
   - **Deny** — reject the command

3. The tool call resolves with the operator's decision.

### Tool: `verify_integrity`

**Description:** Verify WORM (Write-Once-Read-Many) telemetry ledgers for tampering.

**Parameters:** None

**Returns:** Integrity verification results (hash chain validation).

### Tool: `scrub_sensitive`

**Description:** Redact secrets, tokens, and passwords from text before displaying or logging.

| Param | Type | Required | Description |
|---|---|---|---|
| `text` | string | Yes | Text to scrub |

**Returns:** `{ scrubbed_text: "..." }` with secrets replaced by `[REDACTED]`.

Detected patterns include: AWS access keys, GitHub tokens, API keys, passwords in URLs, bearer tokens, private keys.

### Snapshots for Rollback

Managed commands automatically create filesystem snapshots. If something goes wrong:

```
1. list_snapshots() → find the pre-execution snapshot
2. restore_snapshot(snapshot_id="...") → roll back
```

Snapshot backends: tar (default), ZFS, BTRFS (configurable).

## Gotchas

- Not all commands trigger approval — only those matching risk patterns
- `Allow For Session` approves the pattern, not just the exact command — similar commands auto-approve
- Sandbox isolation is configurable and may be disabled in development setups
- Snapshot creation adds latency to managed commands — this is intentional for safety
- WORM ledgers are append-only — they cannot be edited or deleted
- The policy engine uses AST parsing via tree-sitter, not just regex — it understands shell syntax
- Cerbos integration is optional — if configured, it adds Attribute-Based Access Control on top of pattern matching
- External MCP agents see approval_required in the execute_command response — they should surface this to the user
