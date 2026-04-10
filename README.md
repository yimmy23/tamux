![Tamux Slavic Mythology](docs/assets/tamux-slavic.jpg)
# tamux

**Terminal Agentic Multiplexer and TUI with a super-powerful agent** - a daemon-first environment for long-running AI work.
>Tamux is not a CLI tool. It's a full autonomous agent platform. The mental model gap is huge - most people see "terminal multiplexer with AI" but the architecture underneath is a persistent, multi-agent, self-healing, auditable, learning agent
platform.

Official website: [https://tamux.app](https://tamux.app)

tamux keeps the terminal, the agent, and the runtime in one place. Sessions, threads, tasks, approvals, and goal runs live in the daemon, so work can keep moving even when the UI closes.

In practice that means:

- Electron, the TUI, the CLI, MCP clients, and chat gateways all reconnect to the same daemon state
- the built-in runtime can plan work, run tools, spawn bounded sub-agents, pause for approval, and learn over time
- memory, queue state, and operational history stay durable instead of vanishing with a single terminal tab

## What It Feels Like

tamux is for operators who want a terminal that remembers, an agent that can stay with a task, and a control surface that makes long-running work visible instead of mysterious.

It is still a real terminal multiplexer. It just has a daemon beneath it, durable autonomy above it, and enough structure to let automation run without turning into fog.

## The TUI

The TUI is a keyboard-first control room for the daemon with mouse support.

- inspect sessions, threads, tasks, approvals, and goal runs without leaving the terminal
- move between operator control and agent execution from the same live state used by Electron and the CLI
- keep working over SSH, inside tmux, or anywhere a browser UI is the wrong tool

Thread command reference: [`docs/tui/agent-directives.md`](docs/tui/agent-directives.md)

## The Fires

tamux gives its daemon-side agents a slightly mythic face, but the work stays concrete.

- **Swarog** is the main working fire: planning, tool use, sub-agent orchestration, memory, and durable goal runs
- **Rarog** is the guiding flame: onboarding, check-ins, operator context, and the gentler edge of the system
- **Weles** is the underwatch: governance, risk review, and guarded inspection when the runtime needs a second set of eyes

Together they give the system a little presence without hiding what it is doing.

## Quick Start

### NPM

```bash
npm install -g tamux
tamux --help

# or install locally in a project:
npm install tamux
npx tamux --help
```

If `npm install -g tamux` succeeds but `tamux` is still not found on macOS, your npm global bin directory is not on `PATH` yet:

```bash
export PATH="$(npm config get prefix)/bin:$PATH"
exec $SHELL -l
tamux --help
```

If the bin directory is already on `PATH`, opening a new shell is still useful because `zsh` and `bash` can cache command lookups.

### From sources

```bash
git clone https://github.com/mkurman/tamux.git

cd tamux

# 1. Start the daemon
cargo run --release --bin tamux-daemon

# 2. Launch the TUI in another terminal
cargo run --release --bin tamux setup

# or
cargo run --release --bin tamux-tui

# 3. Or launch the desktop app
cd frontend && npm install && npm run dev:electron
```

If you want to test the core loop fast, start a goal run and give Swarog a concrete objective: investigate a failing build, prepare a release checklist, or trace a bug across a workspace.

## Read More

### Documentation

- [Reference](docs/reference.md)
- [Getting Started](docs/getting-started.md)
- [How tamux Works](docs/how-tamux-works.md)
- [Goal Runners](docs/goal-runners.md)
- [Self-Orchestrating Agent Architecture](docs/self-orchestrating-agent.md)
- [Agentic Mission Control](docs/agentic-mission-control.md)
- [TUI Capabilities Map](docs/tui/capabilities-map.md)
- [TUI README](crates/amux-tui/README.md)
- [CDUI YAML Views](docs/cdui-yaml-views.md)
- [Plugin Development](docs/plugin-development.md)

## License

[MIT](LICENSE)
