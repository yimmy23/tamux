# tamux-plugin-gitlab

Minimal GitLab plugin for tamux.

## Scope

Current MVP:
- configure a GitLab token
- fetch project metadata with `/gitlab.repo`
- list open issues with `/gitlab.issues`
- list open merge requests with `/gitlab.mrs`

## Installation

```bash
tamux plugin add tamux-plugin-gitlab
```

## Configuration

Set the `token` setting to a GitLab personal access token with read access to the projects you want to inspect.

## Commands

| Command | Description |
|---|---|
| `/gitlab.repo` | Fetch project details |
| `/gitlab.issues` | List project issues |
| `/gitlab.mrs` | List project merge requests |
