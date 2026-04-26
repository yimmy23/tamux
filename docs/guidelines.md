# tamux Guidelines

Guidelines are local Markdown playbooks that sit above skills. A guideline does not replace a skill and does not execute anything by itself. It tells the agent what kind of task it is handling, which skills are likely relevant, what order to follow, and which failure modes to consider before work starts.

Use guidelines for task-shape decisions such as coding, debugging, research, business strategy, marketing, creative work, data analysis, file management, release work, incident response, user support, writing, scheduling, and agent coordination.

## How Guidelines Work

tamux installs guideline documents into the runtime guidelines directory:

| Platform | Guidelines directory |
|---|---|
| Linux/macOS | `~/.tamux/guidelines` |
| Windows | `%LOCALAPPDATA%\tamux\guidelines` |

At startup and during packaged installation, bundled guidelines are copied into that directory without overwriting user-created files. The guidelines directory is a sibling of the skills directory:

```text
~/.tamux/
  guidelines/
  skills/
```

Agents are prompted to consult guidelines before skill discovery. The intended sequence is:

1. Call `discover_guidelines` with a short intent query.
2. Call `read_guideline` for the best match.
3. Follow the guideline's workflow and recommended skills.
4. Call `discover_skills` when the guideline indicates which detailed procedure is needed.
5. Call `read_skill` for the recommended skill.
6. Execute the task and verify the result.

Guideline discovery uses the same daemon-backed ranking shape as skill discovery. The public result uses the same discovery payload fields, but guideline candidates have `source_kind: "guideline"` and recommend actions like `read_guideline coding-task`.

## Guideline File Format

A guideline is a Markdown file with optional YAML frontmatter:

```markdown
---
name: coding-task
description: Use before implementing a feature, behavior change, or bug fix in code.
recommended_skills:
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
---

# Coding Task Guideline

## Workflow

1. Inspect the current implementation.
2. Build a risk and test matrix.
3. Use test-driven development unless the user asks for another technique.

## Quality Gate

Do not call the task complete until meaningful verification has run.
```

Recommended fields:

| Field | Purpose |
|---|---|
| `name` | Stable lookup name used by `read_guideline` and discovery output. |
| `description` | One-sentence trigger description used for listing and ranking. |
| `recommended_skills` | Skill names that the guideline may route the agent toward. |

The body should stay concise and operational. A useful guideline normally has a short purpose, a workflow, and a quality gate.

## CLI Commands

Install a custom guideline:

```bash
tamux guideline install ./my-guideline.md
```

Install with a different destination filename:

```bash
tamux guideline install ./my-guideline.md --name team-coding.md
```

Overwrite an existing guideline:

```bash
tamux guideline install ./my-guideline.md --force
```

The top-level install command also supports guidelines:

```bash
tamux install guideline ./my-guideline.md
```

List installed guidelines:

```bash
tamux guideline list
tamux guidelines list --json
```

The install command accepts Markdown files only and writes to the canonical runtime guidelines directory. It refuses to overwrite by default.

## MCP And Agent Tools

Agents and MCP clients can use:

| Tool | Purpose |
|---|---|
| `list_guidelines` | Raw catalog view of installed guideline files. |
| `discover_guidelines` | Rank guidelines for a task using daemon-backed discovery. |
| `read_guideline` | Read a guideline by name, stem, or relative path. |

These tools mirror the skills workflow:

| Guidelines | Skills |
|---|---|
| `list_guidelines` | `list_skills` |
| `discover_guidelines` | `discover_skills` |
| `read_guideline` | `read_skill` |

Use `discover_guidelines` for task selection and `list_guidelines` only when you need the raw catalog.

## Bundled Starter Catalog

tamux ships starter guidelines for common everyday work:

- coding, debugging, refactoring, code review, frontend UI, CI failures, and testing
- research, documentation, communication writing, content transformation, and explanation
- academic literature reviews, citations, evidence quality, scientific databases, research data analysis, hypothesis design, research publication, grants, and clinical research
- planning, project management, decision analysis, product design, and task intake
- business strategy, market research, marketing campaigns, brand strategy, sales outreach, customer research, product marketing, and social media
- creative briefs, visual art, design critique, video/audio production, presentation decks, and media generation
- growth experiments, analytics and measurement, customer success, operations, finance and budgeting, legal/compliance issue spotting, HR/recruiting, education/training, events, and fundraising
- git workflow, terminal operations, file management, environment setup, and configuration
- data analysis, spreadsheet/CSV work, automation scripting, database work, and API integration
- security and privacy, incident response, deployment/release, dependency upgrades, plugins, user support, scheduling, media generation, and agent coordination

Users can edit or add their own guidelines in the runtime directory. Packaged updates should add missing bundled defaults without replacing user files.

The non-technical starter guidelines also route to bundled MIT-licensed skills selected from GitHub:

| Source | Bundled coverage |
|---|---|
| [`kostja94/marketing-skills`](https://github.com/kostja94/marketing-skills) | SEO research, copywriting, email marketing, PR, ads, GTM, branding, pricing, visual content, landing pages, and social posts. |
| [`shawnpang/startup-founder-skills`](https://github.com/shawnpang/startup-founder-skills) | Pitch decks, investor updates, market research, competitive analysis, sales outreach, recruiting, operations docs, customer feedback, events, legal/compliance drafting support, and founder communications. |
| [`coreyhaines31/marketingskills`](https://github.com/coreyhaines31/marketingskills) | CRO, analytics, lifecycle email, RevOps, sales enablement, growth loops, social content, SEO audits, and marketing experimentation. |
| [`AbsolutelySkilled/AbsolutelySkilled`](https://github.com/AbsolutelySkilled/AbsolutelySkilled) | Spreadsheets, finance, support operations, customer success, product analytics, project execution, HR, legal operations, video production, and presentation design. |

tamux bundles a focused subset rather than the full upstream catalogs to keep skill discovery useful for day-to-day business, marketing, creative, HR, sales, and operations tasks.

## When To Create A New Guideline

Create a new guideline when a class of work needs a repeatable task-level policy that is broader than a single skill. Good candidates include team-specific release rules, company support process, security review expectations, data handling policy, or project-specific coding workflow.

Do not create a guideline for one-off facts, temporary task notes, or detailed tool instructions that belong in a skill.
