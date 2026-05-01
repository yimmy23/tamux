# zorai-plugin-gitlab

GitLab connector for zorai.

## Scope

This connector now supports daily project-triage loops and normalized repo workflow primitives.

### Readiness / setup
- Required setting: `token`
- Recommended token scope: API access sufficient for issues, merge requests, notes, labels, assignees, and merge operations
- Readiness probe: `check_health`
- TUI plugin settings surface readiness state, setup hint, recovery hint, docs path, normalized primitives, and read/write action inventory

### Supported read actions
- `/gitlab.repo`
- `/gitlab.issues`
- `/gitlab.issue`
- `/gitlab.mrs`
- `/gitlab.mr`

### Supported write actions
- `/gitlab.comment`
- `/gitlab.assign`
- `/gitlab.label`
- `/gitlab.status`
- `/gitlab.merge`

### Normalized workflow primitives
- `list_work_items`
- `fetch_work_item_context`
- `comment_on_work_item`
- `assign_work_item`
- `label_work_item`
- `update_work_item_status`
- `list_review_items`
- `fetch_review_context`
- `comment_on_review_item`
- `merge_review_item`

## Installation

```bash
zorai plugin add zorai-plugin-gitlab
```

## Configuration

Set `token` to a GitLab personal access token with API scope for the projects you want to manage.

## Failure visibility

The readiness + error surfaces now make visible:
- missing setup / missing token
- insufficient permissions or missing scopes
- rate limits
- unreachable GitLab service

Typical recoveries:
- open plugin settings and add the token
- replace token with API-capable scope if writes fail
- retry after rate-limit cooling window
- verify GitLab availability/network access if the service is unreachable

## Repo triage example

1. Confirm readiness via Test Connection.
2. Review open issues with `/gitlab.issues`.
3. Fetch issue context with `/gitlab.issue`.
4. Comment, assign, label, or close using the normalized write actions.
5. Review merge requests with `/gitlab.mrs` and `/gitlab.mr`.
6. Merge with `/gitlab.merge` after approval.

## Commands

| Command | Description |
|---|---|
| `/gitlab.repo` | Fetch project details |
| `/gitlab.issues` | List project issues |
| `/gitlab.issue` | Fetch issue details |
| `/gitlab.mrs` | List merge requests |
| `/gitlab.mr` | Fetch merge request details |
| `/gitlab.comment` | Comment on an issue or MR |
| `/gitlab.assign` | Assign an issue |
| `/gitlab.label` | Add a label to an issue |
| `/gitlab.status` | Update issue state |
| `/gitlab.merge` | Merge a merge request |
