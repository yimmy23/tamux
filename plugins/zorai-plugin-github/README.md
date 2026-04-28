# zorai-plugin-github

Minimal GitHub plugin for zorai.

## Scope

Current MVP:
- configure a GitHub token
- fetch repository metadata with `/github.repo`
- list open issues with `/github.issues`
- list open pull requests with `/github.pulls`

## Installation

```bash
zorai plugin add zorai-plugin-github
```

## Configuration

Set the `token` setting to a GitHub personal access token with read access to the repositories you want to inspect.

## Commands

| Command | Description |
|---|---|
| `/github.repo` | Fetch repository details |
| `/github.issues` | List repository issues |
| `/github.pulls` | List repository pull requests |
