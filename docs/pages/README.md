# tamux GitHub Docs

This folder contains a static HTML documentation site for tamux.

## What it includes

- `index.html` — overview and feature map
- `guides.html` — installation, first-run flow, and operator habits
- `architecture.html` — daemon/runtime/agent deep dive
- `why-tamux.html` — product-positioning explanation of what makes tamux different
- `mission-control.html` — launch/orchestration cockpit for goal runs
- `workspaces.html` — Jira-style workspace task boards over threads and goals
- `goal-runners.html` — durable goal orchestration model
- `task-queue-subagents.html` — daemon execution queue, bounded subagents, and collaboration
- `tui.html` — keyboard-first terminal control plane deep dive
- `gateway-mcp.html` — chat-platform gateways and MCP integration story
- `semantic-learning.html` — semantic environment model, skills, learning, and generated tools
- `moats-intelligence.html` — higher-order capabilities and strategic differentiation
- `memory-security.html` — memory architecture and governance
- `multi-agent.html` — personas, specialization, and shared runtime model
- `threads-handoffs.html` — thread ownership, participants, hidden delegation, and handoffs
- `continuity-provenance.html` — continuity, persistence, and provenance-backed trust
- `liveness-recovery.html` — checkpoints, stuck detection, recovery, and escalation
- `governance.html` — approvals, critique, verdicts, and safety semantics
- `plugins.html` — plugin system and extension model
- `reference.html` — paths, providers, build/run commands, packaging, and publishing notes
- `assets/style.css` — shared site styling
- `assets/site.js` — lightweight navigation behavior
- `.nojekyll` — keeps GitHub Pages from applying Jekyll processing

## Preview locally

### Quick local preview

```bash
cd github-docs
python -m http.server 8000
```

Then open:

- `http://localhost:8000/`

You can also open `index.html` directly in a browser, but an HTTP server is better for testing Pages-like behavior.

## Publish on GitHub Pages

This folder is deliberately static and build-free.

### Option 1 — Copy into `/docs`

If you want GitHub Pages to serve from the main branch `/docs` folder, copy the contents of `github-docs/` into `/docs` (or another Pages-served path configured in your repo).

### Option 2 — Publish from a Pages branch or workflow

Keep authoring in `github-docs/`, then publish that folder’s contents to a Pages branch (for example `gh-pages`) or via a GitHub Actions Pages workflow.

## Design goals

- No mandatory site generator
- Relative links only, so repo-subpath hosting works
- Detailed docs, not a thin README mirror
- Easy to edit manually
