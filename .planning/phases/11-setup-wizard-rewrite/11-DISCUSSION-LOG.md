# Phase 11: Setup Wizard Rewrite - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.

**Date:** 2026-03-24
**Phase:** 11-setup-wizard-rewrite
**Areas discussed:** Config write path, Wizard UX, Provider list, Newcomer security

---

## Config write path

### Bootstrap approach

| Option | Description | Selected |
|--------|-------------|----------|
| Start daemon with defaults first | Daemon starts with no provider, wizard sets via IPC | ✓ |
| Write minimal bootstrap file, then IPC | Two-phase: file then IPC | |
| Wizard embeds daemon logic | Write directly to SQLite | |

**User's choice:** Start daemon with defaults, then IPC

### Re-setup support

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, detect and connect | Works for first-run and reconfiguration | ✓ |
| First-run only | Reconfiguration via settings only | |

**User's choice:** Detect and connect (both first-run and re-setup)

---

## Wizard UX and navigation

### Navigation style

| Option | Description | Selected |
|--------|-------------|----------|
| Arrow-key select lists | Crossterm, like cargo init | ✓ |
| Full ratatui alternate screen | Full TUI rendering | |
| Inquire crate | Third-party prompt library | |

**User's choice:** Arrow-key select lists with crossterm

### Step skippability

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, Esc on each step | All steps skippable | |
| Only optional steps skippable | Provider/key required, rest skippable | ✓ |
| No skipping | Complete everything | |

**User's choice:** Required steps can't be skipped, optional steps can

---

## Provider list and optional steps

### Provider list source

| Option | Description | Selected |
|--------|-------------|----------|
| Query daemon via IPC | Single source of truth, auto-sync | ✓ |
| Hardcoded list | Manual sync needed | |
| Shared config file | File-based single source | |

**User's choice:** Query daemon via IPC

### Optional steps

| Option | Description | Selected |
|--------|-------------|----------|
| Web search tool API key | Firecrawl/exa/tavily | ✓ |
| Gateway setup | Slack/Discord/Telegram | ✓ |
| Default model selection | Pick model from provider | ✓ |
| Data directory customization | ~/.tamux/ or custom | ✓ |

**User's choice:** All four optional steps included

---

## Newcomer security defaults

### Security level

| Option | Description | Selected |
|--------|-------------|----------|
| Approve everything | ALL actions need approval for newcomers | ✓ |
| Approve only dangerous actions | Safe tools auto-approved | |
| Sandbox mode | Simulate-then-execute | |

**User's choice:** Approve everything for newcomers

### Security question in wizard

| Option | Description | Selected |
|--------|-------------|----------|
| No — derive from tier | Tier determines security | |
| Yes — separate question | Independent from tier | ✓ |
| You decide | Claude's discretion | |

**User's choice:** Separate security preference question in wizard

---

## Claude's Discretion

- Crossterm rendering details
- Specific IPC messages per config item
- Daemon startup timeout handling
- Connectivity test implementation
- Security option wording

## Deferred Ideas

- Electron wizard variant
- OAuth flow for providers
- Cross-machine config import
