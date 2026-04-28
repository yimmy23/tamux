---
name: github
description: >
  Use when the user asks about a GitHub repository, open issues, pull requests,
  repo metadata, or wants a quick repository status summary using the GitHub
  plugin.
---

# GitHub Plugin

Use the **GitHub plugin** for read-only repository checks.

## Endpoints

| Endpoint | Method | Use |
|---|---|---|
| `get_repo` | GET | Repository metadata and summary |
| `list_issues` | GET | Open or filtered issues |
| `list_pull_requests` | GET | Open or filtered pull requests |

## Core calls

Repository summary:

```json
{"plugin_name": "github", "endpoint_name": "get_repo", "params": {"owner": "anthropic", "repo": "cmux-next"}}
```

List open issues:

```json
{"plugin_name": "github", "endpoint_name": "list_issues", "params": {"owner": "anthropic", "repo": "cmux-next"}}
```

List pull requests:

```json
{"plugin_name": "github", "endpoint_name": "list_pull_requests", "params": {"owner": "anthropic", "repo": "cmux-next"}}
```

## Optional filters

Issues and pull requests support:
- `state` — default `open`
- `per_page` — default `20`

Example:

```json
{"plugin_name": "github", "endpoint_name": "list_pull_requests", "params": {"owner": "anthropic", "repo": "cmux-next", "state": "closed", "per_page": 10}}
```

## Usage guidance

- Use `get_repo` first when the user wants a high-level repo overview.
- Use `list_issues` when the user asks about backlog, bugs, or open work.
- Use `list_pull_requests` when the user asks about review queue or active changes.
- Summarize results clearly instead of dumping raw JSON.

## Error handling

If the plugin is not configured, direct the user to set the GitHub plugin `token` in plugin settings.
