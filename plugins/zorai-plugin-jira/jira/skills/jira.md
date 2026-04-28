---
name: jira
description: >
  Use when the user asks about Jira issues, project lists, issue keys, or wants
  a quick tracker summary using the Jira plugin.
---

# Jira Plugin

Use the **Jira plugin** for read-only issue and project checks.

## Endpoints

| Endpoint | Method | Use |
|---|---|---|
| `get_issue` | GET | Fetch one issue by key |
| `list_issues` | GET | List issues using JQL |
| `list_projects` | GET | List projects |

## Core calls

Issue summary:

```json
{"plugin_name": "jira", "endpoint_name": "get_issue", "params": {"issue_key": "ENG-123"}}
```

List issues:

```json
{"plugin_name": "jira", "endpoint_name": "list_issues", "params": {"jql": "project = ENG ORDER BY created DESC", "max_results": 10}}
```

List projects:

```json
{"plugin_name": "jira", "endpoint_name": "list_projects", "params": {"max_results": 10}}
```

## Optional filters

Current MVP supports:
- `issue_key` for `get_issue`
- `jql` and `max_results` for `list_issues`
- `max_results` for `list_projects`

## Usage guidance

- Use `get_issue` when the user references a specific Jira issue like `ENG-123`.
- Use `list_issues` when the user wants backlog or queue visibility.
- Use `list_projects` when the user wants workspace/project context.
- Summarize results clearly instead of dumping raw JSON.

## Configuration note

This plugin expects:
- `site` = Jira host like `your-company.atlassian.net`
- `auth_header` = full Authorization header value such as `Basic ...` or `Bearer ...`
