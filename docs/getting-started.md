# tamux Getting Started

tamux is an AI-native terminal multiplexer with workspaces, panes, a Rust daemon, and an Electron desktop UI. The daemon now also owns long-running goal runners that can plan work, enqueue child tasks, pause for approval, and persist what they learn.

For providers, configuration, shortcuts, release packaging, plugin installation, and MCP registration, see [reference.md](reference.md).

## Install Locations

- **Desktop app binaries (packaged):** inside application resources (`resources/bin` on desktop builds).
- **Runtime data directory:** `~/.tamux` (Unix) or `%LOCALAPPDATA%\tamux` (Windows).
- **Source builds:** binaries are produced in `target/debug` or `target/release`.

## Quick Install

Use the shell installer when you want the native CLI binaries without npm:

```bash
curl -fsSL https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.sh | sh
tamux --help
```

The installer downloads the same GitHub release bundles used by the npm package and installs them into `~/.local/bin` by default. Set `TAMUX_VERSION` to pin a specific release or `TAMUX_INSTALL_DIR` to change the install location.

Later, `tamux upgrade` will upgrade from the same source that installed the current binary: npm-backed installs use npm, and direct-binary installs reuse the direct installer for the active install directory.

## Required Dependencies

### Source / Development workflow

- Rust toolchain (`cargo`)
- Node.js + npm
- git
- uv

### Packaged desktop runtime

- No extra hard-required dependencies.

## Optional (Recommended) Integrations

- aline (used for OneContext history recall and richer agent memory/bootstrap behavior)

## Setup Preflight

Run preflight checks before building or running:

```bash
./scripts/setup.sh --check --profile source
```

Windows PowerShell:

```powershell
.\scripts\setup.ps1 -Check -Profile source
```

Desktop-only runtime preflight:

```bash
./scripts/setup.sh --check --profile desktop
```

## Install Hints

- Install `uv`: `curl -LsSf https://astral.sh/uv/install.sh | sh`
- Install `aline`: `uv tool install aline-ai`

On Windows:

- Install Node.js LTS: `winget install OpenJS.NodeJS.LTS`
- Install Rust: `winget install Rustlang.Rustup`
- Install git: `winget install Git.Git`

## First Goal Run

Use this path if you want to validate the long-running autonomy flow end to end.

1. Run the source preflight checks.
2. Start the daemon:

```bash
cargo run --release --bin tamux-daemon
```

3. In another terminal, launch the desktop app:

```bash
cd frontend
npm run dev:electron
```

4. Open the agent panel and confirm the backend is set to `daemon`.
5. Open the `Goal Runners` view.
6. Enter a long-running objective, for example:

```text
Investigate why the nightly Rust build is failing, summarize the cause, and capture any reusable workflow as a skill.
```

7. Observe the lifecycle:
   - the goal enters `queued`
   - the daemon generates a plan
   - child tasks are enqueued and executed
   - approvals pause risky managed commands
   - the final run records a reflection, optional memory update, and optional generated skill

## Current Limits

- Goal runners currently use the built-in `daemon` backend.
- `pause` stops further orchestration, but it does not forcibly kill a child task that is already running.
- Memory updates are intended for durable facts or operator preferences, not transient run output.
