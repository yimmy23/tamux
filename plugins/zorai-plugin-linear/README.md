# zorai-plugin-linear

Linear connector for zorai.

## Scope

This connector now covers the core daily tracker loop for Linear work.

### Readiness / setup
- Required setting: `token`
- Recommended token capability: issue read/write access
- Readiness probe: `check_health`

### Supported read actions
- `/linear.issue`
- `/linear.issues`
- `/linear.projects`
- `/linear.work` (normalized work-item context)

### Supported write actions
- `/linear.create`
- `/linear.comment`
- `/linear.assign`
- `/linear.transition`

### Normalized workflow primitives
- `list_work_items`
- `fetch_work_item_context`
- `create_work_item`
- `comment_on_work_item`
- `assign_work_item`
- `update_work_item_status`

## Installation

```bash
zorai plugin add zorai-plugin-linear
```

## Configuration

Set `token` to a Linear API token with read/write access to the workspace.

## Failure visibility

Readiness and enriched error messages now surface:
- missing setup / missing token
- permission or scope problems
- rate limits
- unreachable Linear API

## Project-loop example

1. Test readiness.
2. List work with `/linear.issues`.
3. Open normalized context with `/linear.work`.
4. Create, comment, assign, or transition the issue as needed.

## Commands

| Command | Description |
|---|---|
| `/linear.issue` | Fetch issue details by identifier |
| `/linear.issues` | List recent issues |
| `/linear.projects` | List projects |
| `/linear.work` | Fetch normalized work-item context |
| `/linear.create` | Create a Linear issue |
| `/linear.comment` | Comment on a Linear issue |
| `/linear.assign` | Assign a Linear issue |
| `/linear.transition` | Move a Linear issue to a new state |
