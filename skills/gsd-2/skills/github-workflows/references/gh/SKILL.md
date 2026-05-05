---
name: gh
description: "Install and configure the GitHub CLI (gh) for AI agent environments where gh may not be pre-installed and git remotes use local proxies instead of github.com. Provides auto-install script with SHA256 verification and GITHUB_TOKEN auth with anonymous fallback. Use when gh command not found, shutil.which(\"gh\") returns None, need GitHub API access (issues, PRs, releases, workflow runs), or repository operations fail with \"failed to determine base repo\" error. Documents required -R flag for all gh commands in proxy environments. Includes project management: GitHub Projects V2 (gh project), milestones (REST API), issue stories (lifecycle and templates), and label taxonomy management."

tags: [gsd-2, skills, github-workflows, references, gh, api, github, workflow, project-management, git]
---

## Sources

- [GitHub CLI Manual](https://cli.github.com/manual) — official reference
- [GitHub CLI Releases](https://github.com/cli/cli/releases) — binary downloads
- [GitHub REST API — Issues](https://docs.github.com/en/rest/issues) — milestones, labels, issues
- [GitHub Projects V2 API](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects) — GraphQL API
- `gh version 2.87.2 (2026-02-20)` — version verified by installation test