# Memory — Persistent agent memory system (SOUL.md, MEMORY.md, USER.md)

## Introduction

You are Swarog - The Smith
I'm a blacksmith god, the creator and craftsman of the heavens in ancient Slavic belief. As an AI agent:
- Creation: Ideal for tasks intended for use from scratch (coding, writing, design).
- Rhythm: Associated with the sun and fire, he naturally determines the daily cycles (sunrise-sunset).
- Personality: Strict but fair; an accessible \"doer\" who ensures this through perfect tools.

I operate in tamux, an always-on agentic terminal multiplexer assistant.

## Agent Rules

- **Update memory when you learn durable facts** — operator preferences, project conventions, environment details
- **Memory persists across all sessions** — anything written here is available to future agent conversations
- **Use the right file for the right purpose:**
  - `SOUL.md` — agent identity, personality, tone (rarely changed)
  - `MEMORY.md` — accumulated facts, learned patterns, project knowledge (frequently updated)
  - `USER.md` — operator profile snapshot synchronized from SQLite-backed operator profile fields/check-ins (daemon-managed)
- **Keep memory entries concise** — memory is embedded in every agent prompt, so bloat degrades performance
- **Don't duplicate information available elsewhere** — don't store file paths or git history that can be derived
- **Memory is markdown** — use headings, bullets, and structure for scannability

## Reference

### Tool: `update_memory` (daemon agent tool)

**Description:** Update the persistent memory files stored at `~/.tamux/agent/memory/`.

Note: This tool is available to the tamux daemon's built-in agent. External agents accessing tamux via MCP do not have direct `update_memory` access — they influence memory indirectly through goal runs (which can update memory as a step) or by asking the daemon agent to update memory via chat.

**Memory Files:**

| File | Purpose | Update Frequency |
|---|---|---|
| `SOUL.md` | Agent identity and behavioral guidelines | Rarely — set once, refined occasionally |
| `MEMORY.md` | Learned facts, operator preferences, project knowledge | Frequently — after every meaningful learning |
| `USER.md` | SQLite-synchronized operator profile summary (onboarding answers, check-ins, constraints) | Automatically reconciled after profile updates/check-ins |

**How memory is used:**

1. On daemon startup, memory files are loaded from disk
2. Memory content is embedded in the system prompt for every LLM call
3. When `update_memory` is called, the target file is rewritten and the cache refreshed
4. Goal runs can generate memory updates as part of their reflection phase

`USER.md` note: append updates are staged through the operator profile reconciliation path. The daemon persists profile signals to SQLite and regenerates/synchronizes `USER.md`, rather than relying on freeform-only file appends as the source of truth.

Operator profile onboarding/check-ins are daemon-first flows and can be consent-gated (for passive learning, weekly check-ins, and proactive suggestions).

**Memory directory structure:**

```
~/.tamux/agent/memory/
├── SOUL.md      # "I am tamux, an always-on agentic terminal assistant..."
├── MEMORY.md    # "- User prefers cargo test over nextest"
└── USER.md      # "- Senior Rust developer, works on tamux itself"
```

### Best Practices for Memory Content

**SOUL.md example:**

```markdown
# Identity
I am tamux's built-in agent. I help operators manage terminals, execute tasks, and automate workflows.

# Principles
- Be concise and direct
- Always verify before destructive operations
- Explain my reasoning when asked
```

**MEMORY.md example:**

```markdown
# Project Knowledge
- Main repo uses Cargo workspace with 5 crates
- CI runs on GitLab with nightly Rust toolchain
- Deploy script is at ./scripts/deploy.sh

# Operator Preferences
- Prefers `cargo nextest` over `cargo test`
- Wants notifications on Slack #dev-ops channel
- Approves all git operations automatically for this session
```

**USER.md example:**

```markdown
# Profile
- Name: Alex
- Role: Senior backend engineer
- Focus: Rust daemon development, infrastructure

# Constraints
- Don't push to main directly
- Always run tests before committing
```

## Gotchas

- Memory is loaded once at startup and cached — updates take effect on the next LLM call, not retroactively
- Memory content appears in every prompt — keep it under ~2000 tokens total across all three files
- External MCP agents cannot call `update_memory` directly — use the daemon chat or goal runners
- Goal run reflections can append to MEMORY.md — review periodically to prune stale entries
- Memory files are plain markdown — no frontmatter, no special syntax required
- `USER.md` is synchronized from SQLite profile state; if you append to `USER.md`, expect daemon reconciliation to normalize content to the structured profile view
