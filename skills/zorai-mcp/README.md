# zorai MCP Skills Library

> Skill library for zorai, the daemon-first agentic runtime that exposes terminal, browser, task, goal, memory, and messaging capabilities via MCP.

## What is zorai?

zorai is a daemon-first agentic runtime purpose-built for long-running AI workflows. It wraps persistent shell-backed sessions, a headless browser, a background task queue, durable goal runners, and agent memory into a single daemon process, then exposes every capability as an MCP tool. Any MCP-compatible agent -- Claude Code, Cursor, or a custom integration -- can connect to zorai and orchestrate shell commands, web browsing, multi-step objectives, and cross-session knowledge without losing state between conversations.

## Quick Start

1. **Install zorai-mcp**
   ```bash
   cargo install zorai-mcp
   ```

2. **Register in your agent**
   Add the zorai MCP server to your agent's MCP configuration. For Claude Code, add an entry to `.mcp.json`:
   ```json
   {
     "mcpServers": {
       "zorai": {
         "command": "zorai-mcp"
       }
     }
   }
   ```

3. **Verify the connection**
   Ask your agent to call `list_sessions`. If it returns a session list (even an empty one), zorai is connected and ready.

## Skills Index

### Getting Started

- [connection/setup.md](connection/setup.md) -- Connect zorai-mcp to Claude Code, Cursor, or any MCP client
- [setup/install-plugins.md](setup/install-plugins.md) -- Install zorai runtime plugins and verify daemon registration
- [setup/install-skills.md](setup/install-skills.md) -- Import community skills and verify local skill availability

### Operating zorai

- [operating/terminals.md](operating/terminals.md) -- Execute commands, read terminal output, interactive input
- [operating/browser.md](operating/browser.md) -- Canvas browser panels, DOM reading, clicking, typing, JS eval
- [operating/synthlabs.md](operating/synthlabs.md) -- Choose the right SynthLabs workflow skill for setup, generation, curation, and UI-led tasks
- [operating/tasks.md](operating/tasks.md) -- Background task queue with dependencies and scheduling
- [operating/goals.md](operating/goals.md) -- Durable goal runners for multi-step autonomous objectives
- [operating/memory.md](operating/memory.md) -- Persistent agent memory (SOUL.md, MEMORY.md, USER.md)
- [operating/workspaces.md](operating/workspaces.md) -- Workspace layout, surfaces, pane splits, snippets
- [operating/safety.md](operating/safety.md) -- Approval workflows, risk policies, sandbox behavior
- [operating/messaging.md](operating/messaging.md) -- Send/receive via Slack, Discord, Telegram
- [operating/observability.md](operating/observability.md) -- History search, snapshots, symbol search, git status

### Building on zorai

- [building/plugin-development.md](building/plugin-development.md) -- Create plugins with custom tools, views, and components

## Which Skill Do I Need?

| I want to...                              | Read this                                                      |
| ----------------------------------------- | -------------------------------------------------------------- |
| Connect my agent to zorai                 | [connection/setup.md](connection/setup.md)                     |
| Install a zorai runtime plugin            | [setup/install-plugins.md](setup/install-plugins.md)           |
| Import a community skill                  | [setup/install-skills.md](setup/install-skills.md)             |
| Run shell commands                        | [operating/terminals.md](operating/terminals.md)               |
| Browse the web or scrape pages            | [operating/browser.md](operating/browser.md)                   |
| Choose a SynthLabs workflow for your task | [operating/synthlabs.md](operating/synthlabs.md)               |
| Queue background work                     | [operating/tasks.md](operating/tasks.md)                       |
| Run a multi-step autonomous objective     | [operating/goals.md](operating/goals.md)                       |
| Persist knowledge across sessions         | [operating/memory.md](operating/memory.md)                     |
| Arrange terminal layout                   | [operating/workspaces.md](operating/workspaces.md)             |
| Understand approval prompts               | [operating/safety.md](operating/safety.md)                     |
| Send a Slack, Discord, or Telegram message| [operating/messaging.md](operating/messaging.md)               |
| Search command history or code symbols    | [operating/observability.md](operating/observability.md)       |
| Build a zorai plugin                      | [building/plugin-development.md](building/plugin-development.md) |

## Context7 Integration

To index this skill library with context7, point the resolver at this directory:

```jsonc
// In your context7 configuration
{
  "libraries": [
    {
      "name": "zorai",
      "path": "skills",
      "entrypoint": "README.md"
    }
  ]
}
```

context7 will crawl the README, follow the relative links to each skill file, and make the full library searchable via `resolve-library-id` and `query-docs`.

## Cheatsheet

See [cheatsheet.md](cheatsheet.md) for a quick reference of all tools.
