---
name: linear
description: >
  Use when the user asks about Linear issues, project lists, issue identifiers,
  or wants a quick tracker summary using the Linear plugin.
---

# Linear Plugin

Use the **Linear plugin** for read-only issue and project checks.

## Endpoints

| Endpoint | Method | Use |
|---|---|---|
| `get_issue` | POST | Fetch one issue by identifier |
| `list_issues` | POST | List recent issues |
| `list_projects` | POST | List projects |

## Core calls

Issue summary:

```json
{"plugin_name": "linear", "endpoint_name": "get_issue", "params": {"identifier": "ENG-123"}}
```

List issues:

```json
{"plugin_name": "linear", "endpoint_name": "list_issues", "params": {"first": 10}}
```

List projects:

```json
{"plugin_name": "linear", "endpoint_name": "list_projects", "params": {"first": 10}}
```

## Optional filters

Current MVP supports:
- `identifier` for `get_issue`
- `first` for `list_issues` and `list_projects` (default `20`)

## Usage guidance

- Use `get_issue` when the user references a specific Linear issue like `ENG-123`.
- Use `list_issues` when the user wants a quick view of recent work items.
- Use `list_projects` when the user wants active project context.
- Summarize results clearly instead of dumping raw GraphQL output.

## Error handling

If the plugin is not configured, direct the user to set the Linear plugin `token` in plugin settings.
