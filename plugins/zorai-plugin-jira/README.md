# zorai-plugin-jira

Jira connector for zorai.

## Scope

This connector now supports the core daily work-tracker loop for Jira.

### Readiness / setup
- Required settings:
  - `site`
  - `auth_header`
- Recommended capability: issue read/write permissions for the target Jira project(s)
- Readiness probe: `check_health`

### Supported read actions
- `/jira.issue`
- `/jira.issues`
- `/jira.projects`
- `/jira.work` (normalized work-item context)

### Supported write actions
- `/jira.create`
- `/jira.comment`
- `/jira.assign`
- `/jira.transition`

### Normalized workflow primitives
- `list_work_items`
- `fetch_work_item_context`
- `create_work_item`
- `comment_on_work_item`
- `assign_work_item`
- `update_work_item_status`

## Installation

```bash
zorai plugin add zorai-plugin-jira
```

## Configuration

Set:
- `site` to your Jira host, e.g. `your-company.atlassian.net`
- `auth_header` to the full authorization header value required by your Jira deployment

## Failure visibility

Readiness and enriched API errors now surface:
- missing setup / missing required settings
- permission or scope issues
- rate limits
- unreachable Jira service

## Project-loop example

1. Confirm readiness.
2. List issues with `/jira.issues`.
3. Fetch normalized context with `/jira.work`.
4. Create, comment, assign, or transition the issue as needed.

## Commands

| Command | Description |
|---|---|
| `/jira.issue` | Fetch issue details by key |
| `/jira.issues` | List issues using JQL |
| `/jira.projects` | List projects |
| `/jira.work` | Fetch normalized work-item context |
| `/jira.create` | Create a Jira issue |
| `/jira.comment` | Comment on a Jira issue |
| `/jira.assign` | Assign a Jira issue |
| `/jira.transition` | Move a Jira issue through a transition |
