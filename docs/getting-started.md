# Zorai Getting Started

Zorai is a fully agentic daemon runtime with workspaces, a Rust daemon, and an Electron desktop UI. The daemon owns agent threads, workspace task boards, long-running goal runners, approvals, memory, and tool execution so work can continue beyond any single UI session.

For providers, configuration, shortcuts, release packaging, plugin installation, and MCP registration, see [reference.md](reference.md).

## Install Locations

- **Desktop app binaries (packaged):** inside application resources (`resources/bin` on desktop builds).
- **Runtime data directory:** `~/.zorai` (Unix) or `%LOCALAPPDATA%\zorai` (Windows).
- **Source builds:** binaries are produced in `target/debug` or `target/release`.

## Quick Install

Use the native installer when you want the full platform bundle without npm.

Linux/macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.sh | sh
zorai --help
```

Windows PowerShell, run as Administrator:

```powershell
irm https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.ps1 | iex
zorai --help
zorai gui
```

The installer downloads the same GitHub release bundles used by the npm package and installs them into `~/.local/bin` on Linux/macOS or `C:\Program Files\zorai` on Windows by default. Set `ZORAI_VERSION` to pin a specific release or `ZORAI_INSTALL_DIR` to change the install location.

Every quick-install and npm install uses the full platform bundle. Windows x64 and Windows ARM64 download architecture-matched bundles that include `zorai.exe`, `zoi.exe`, daemon/TUI/MCP/gateway binaries, and `zorai-desktop.exe`, so `zorai gui` works after install.

Later, `zorai upgrade` will upgrade from the same source that installed the current binary: npm-backed installs use npm, and direct-binary installs reuse the direct installer for the active install directory.

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
cargo run --release --bin zorai-daemon
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
   - child execution entries are enqueued and executed
   - approvals pause risky managed commands
   - the final run records a reflection, optional memory update, and optional generated skill

## Current Limits

- Goal runners currently use the built-in `daemon` backend.
- `pause` stops further orchestration, but it does not forcibly kill a child execution entry that is already running.
- Memory updates are intended for durable facts or operator preferences, not transient run output.
