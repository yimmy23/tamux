---
name: gitlab
description: >
  Use when the user asks about a GitLab project, open issues, merge requests,
  project metadata, or wants a quick project status summary using the GitLab
  plugin.
---

# GitLab Plugin

Use the **GitLab plugin** for read-only project checks.

## Endpoints

| Endpoint | Method | Use |
|---|---|---|
| `get_repo` | GET | Project metadata and summary |
| `list_issues` | GET | Open or filtered issues |
| `list_merge_requests` | GET | Open or filtered merge requests |

## Core calls

Project summary:

```json
{"plugin_name": "gitlab", "endpoint_name": "get_repo", "params": {"project": "group/project"}}
```

List open issues:

```json
{"plugin_name": "gitlab", "endpoint_name": "list_issues", "params": {"project": "group/project"}}
```

List merge requests:

```json
{"plugin_name": "gitlab", "endpoint_name": "list_merge_requests", "params": {"project": "group/project"}}
```

## Optional filters

Issues and merge requests support:
- `state` — default `opened`
- `per_page` — default `20`

Example:

```json
{"plugin_name": "gitlab", "endpoint_name": "list_merge_requests", "params": {"project": "group/project", "state": "merged", "per_page": 10}}
```

## Usage guidance

- Use `get_repo` first when the user wants a high-level project overview.
- Use `list_issues` when the user asks about backlog, bugs, or open work.
- Use `list_merge_requests` when the user asks about review queue or active changes.
- Summarize results clearly instead of dumping raw JSON.

## Error handling

If the plugin is not configured, direct the user to set the GitLab plugin `token` in plugin settings.
