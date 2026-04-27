# tamux-plugin-jira

Minimal Jira plugin for tamux.

## Scope

Current MVP:
- configure a Jira site host and authorization header
- fetch issue details with `/jira.issue`
- list issues with `/jira.issues`
- list projects with `/jira.projects`

## Installation

```bash
tamux plugin add tamux-plugin-jira
```

## Configuration

Set:
- `site` to your Jira host, for example `your-company.atlassian.net`
- `auth_header` to the full Authorization header value required by your Jira deployment

## Commands

| Command | Description |
|---|---|
| `/jira.issue` | Fetch issue details by key |
| `/jira.issues` | List issues using JQL |
| `/jira.projects` | List projects |
