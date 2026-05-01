# zorai-plugin-github

GitHub connector for zorai.

## Scope

This connector now covers daily repo-triage loops, not just read-only inspection.

### Readiness / setup
- Required setting: `token`
- Recommended token scope: repository read/write access sufficient for issues, pull requests, comments, labels, assignees, and merge operations
- Readiness probe: `check_health`
- TUI plugin settings now surface readiness state, setup hint, recovery hint, docs path, normalized workflow primitives, and read/write action inventory

### Supported read actions
- `/github.repo` → repository metadata
- `/github.issues` → issue list
- `/github.issue` → issue context via normalized work-item primitive
- `/github.pulls` → PR list
- `/github.pull` → PR context

### Supported write actions
- `/github.comment` → comment on an issue or PR
- `/github.assign` → assign an issue
- `/github.label` → add a label to an issue
- `/github.status` → close/reopen issue state through normalized status updates
- `/github.merge` → merge a PR

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
zorai plugin add zorai-plugin-github
```

## Configuration

Set `token` to a GitHub personal access token with the repo permissions needed for the actions above.

## Failure visibility

The connector readiness surface and plugin API errors now distinguish:
- missing setup / missing token
- expired or reconnect-needed auth
- insufficient scopes or permissions
- rate limits
- unreachable service / timeout

Typical recoveries:
- missing setup → open plugin settings and add the token
- insufficient permissions → reconnect or replace the token with repo write access
- rate limit → retry later / reduce polling frequency
- unreachable service → verify GitHub availability and local network access

## Repo triage example

1. Test readiness from Settings → Plugins or call the readiness endpoint indirectly through the Test Connection button.
2. List open work items with `/github.issues`.
3. Fetch detailed context for a specific item with `/github.issue`.
4. Comment or label using `/github.comment` or `/github.label`.
5. Review open PRs with `/github.pulls` and `/github.pull`.
6. Merge with `/github.merge` after approval.

## Commands

| Command | Description |
|---|---|
| `/github.repo` | Fetch repository details |
| `/github.issues` | List repository issues |
| `/github.issue` | Fetch issue details |
| `/github.pulls` | List repository pull requests |
| `/github.pull` | Fetch pull request details |
| `/github.comment` | Comment on an issue or PR |
| `/github.assign` | Assign an issue |
| `/github.label` | Add a label to an issue |
| `/github.status` | Update issue state |
| `/github.merge` | Merge a pull request |
