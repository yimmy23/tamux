# zorai-plugin-linear

Minimal Linear plugin for zorai.

## Scope

Current MVP:
- configure a Linear API token
- fetch issue details with `/linear.issue`
- list recent issues with `/linear.issues`
- list projects with `/linear.projects`

## Installation

```bash
zorai plugin add zorai-plugin-linear
```

## Configuration

Set the `token` setting to a Linear API token with read access to the workspace you want to inspect.

## Commands

| Command | Description |
|---|---|
| `/linear.issue` | Fetch issue details by identifier |
| `/linear.issues` | List recent issues |
| `/linear.projects` | List projects |
