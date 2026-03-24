# Phase 9: Distribution - Research

**Researched:** 2026-03-24
**Domain:** Cross-platform binary distribution, npm packaging, CI/CD release automation, install scripts, first-run bootstrapping
**Confidence:** HIGH

## Summary

Phase 9 delivers the install-and-run story for tamux: anyone on Linux, macOS, or Windows can install with a single command and be running within seconds. The phase covers four delivery channels (npm wrapper package, standalone binaries via GitLab Releases, Unix install script, PowerShell installer) and a first-run setup wizard in the CLI/TUI.

The project already has a mature build infrastructure (`build-release.sh` with `--target` cross-compilation, `build-release-wsl.sh` for Windows cross-build, `build-production-releases.sh` orchestrator, `bump-version.sh` for synchronized version management, `setup.sh` for dependency checking). The primary work is: (1) a thin npm wrapper package with postinstall binary download, (2) a GitLab CI pipeline for automated multi-platform builds, (3) install scripts for curl-pipe and PowerShell channels, and (4) a terminal-based setup wizard for first-run configuration.

**Primary recommendation:** Build on the existing `build-release.sh` infrastructure. The npm package follows the esbuild/Sentry postinstall-download pattern (not optionalDependencies, per decision D-01). GitLab CI uses matrix jobs with the `release` keyword and Generic Package Registry for artifact hosting. The setup wizard lives in the CLI crate and writes to `~/.tamux/agent/config.json` via the existing `AgentConfig` structure.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Thin npm package with a postinstall script that detects OS/arch and downloads pre-built binaries from GitLab Releases. Pattern follows esbuild/turbo/prisma.
- **D-02:** The npm package delivers ALL binaries: daemon (`tamux-daemon`), CLI (`tamux`), TUI (`tamux-tui`), and Electron desktop app archive. Full experience from a single `npx tamux`.
- **D-03:** Download source is GitLab Releases (project release artifacts). URL pattern: `gitlab.com/{org}/tamux/-/releases/v{version}/downloads/tamux-{target}.tar.gz`
- **D-04:** `npx tamux` with no prior install downloads binaries, starts daemon, and opens TUI in the current terminal. TUI is the default surface for `npx` since it works everywhere including SSH.
- **D-05:** GitLab CI matrix job for cross-platform builds. Linux runner for x64/arm64, macOS runner for universal binary, Windows runner for x64. Leverages existing `build-release.sh` with `--target` support.
- **D-06:** All four platform targets built: linux-x64, linux-arm64, darwin-arm64 + darwin-x64 (universal binary), windows-x64.
- **D-07:** Release artifacts split into two categories per platform: binaries tarball (daemon + CLI + TUI) and Electron app archive (AppImage/deb, dmg/zip, NSIS/portable). Users can grab just CLI tools without the desktop app.
- **D-08:** Naming convention: `tamux-binaries-v{version}-{platform}-{arch}.tar.gz` and `tamux-desktop-v{version}-{platform}-{arch}.{ext}`. SHA256SUMS file alongside.
- **D-09:** Automated release pipeline: pushing a `vX.Y.Z` tag triggers CI build, uploads artifacts to GitLab Releases, and publishes the npm wrapper package. Fully automated.
- **D-10:** Unix install script (`install.sh`) places binaries in `~/.local/bin`. XDG-conventional, no sudo required. Script adds to PATH via shell profile if not already present.
- **D-11:** Install script downloads daemon + CLI + TUI by default. Offers optional prompt to also install the Electron desktop app: "Install desktop app? [y/N]". Keeps default install lean for servers/containers.
- **D-12:** PowerShell installer places binaries in `C:\Program Files\tamux`. Requires admin elevation. Standard Windows install location.
- **D-13:** SHA256 checksum verification on all downloads. Download SHA256SUMS file, verify before extracting. Warn but continue if sha256sum utility not available.
- **D-14:** First run triggers an interactive terminal setup wizard before starting the agent. Structured guided experience: provider selection, API key, preferences.
- **D-15:** Setup wizard covers four steps: (1) LLM provider selection + API key entry from 20+ supported providers, (2) default client preference (TUI vs Electron), (3) data directory confirmation (~/.tamux/ or custom), (4) quick connectivity test to verify LLM connection works.
- **D-16:** Scope boundary with Phase 10: Phase 9 handles daemon auto-start + setup wizard (provider, client, data dir, test). Phase 10 handles capability tiers, guided tours, progressive feature disclosure, and concierge onboarding.

### Claude's Discretion
- Setup wizard visual design and terminal formatting
- Exact postinstall script error handling and fallback behavior
- CI pipeline job naming and stage organization
- Whether to generate a shell completion script during install
- Electron app packaging details per platform (existing electron-builder config is a good starting point)

### Deferred Ideas (OUT OF SCOPE)
- Homebrew tap / formula for macOS
- Scoop/Chocolatey/winget manifests for Windows package managers
- Docker image for containerized deployment
- Auto-update mechanism (check for new versions, offer to update)
- AUR package for Arch Linux
- Snap/Flatpak packages
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DIST-01 | Single-command install via npm: `npx tamux` downloads and runs appropriate platform binary | npm postinstall pattern (Sentry/esbuild model), platform detection via `process.platform`/`process.arch`, binary download from GitLab Releases |
| DIST-02 | Standalone platform binaries via cargo-dist (Linux, macOS, Windows) | GitLab CI matrix builds with existing `build-release.sh --target`; `cross` tool for linux-arm64; naming convention per D-08 |
| DIST-03 | Shell installer: `curl -fsSL https://tamux.dev/install.sh \| sh` for Unix systems | Install script targeting `~/.local/bin`, PATH auto-detection and shell profile patching |
| DIST-04 | PowerShell installer for Windows | PowerShell script downloading from same GitLab Releases, placing in `C:\Program Files\tamux`, PATH update via `[Environment]::SetEnvironmentVariable` |
| DIST-05 | First-run automatically spawns daemon, opens default client, and starts concierge onboarding | First-run detection via missing `~/.tamux/agent/config.json`, setup wizard in CLI crate, daemon auto-start via existing `tamux daemon` command |
</phase_requirements>

## Standard Stack

### Core
| Library/Tool | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Node.js `process.platform`/`process.arch` | Built-in | Platform detection in npm postinstall | Zero dependency, reliable, used by esbuild/turbo/prisma |
| `cross` (cross-rs) | Latest | Linux ARM64 cross-compilation from x86_64 | Zero-setup cross-compilation using Docker; handles aarch64-unknown-linux-gnu linker/sysroot |
| GitLab CI `release` keyword | Current | Automated release creation on tag push | Native GitLab feature, no external tooling needed |
| GitLab Generic Package Registry | Current | Binary artifact hosting for releases | Free for project, integrates with CI job tokens |
| `glab` CLI | v1.58.0+ | Release creation from CI pipelines | Replaces deprecated `release-cli`, official GitLab tool |
| `ratatui` | 0.29 | Setup wizard TUI rendering (already in project) | Already used for tamux-tui; consistent visual identity |
| `crossterm` | 0.28 | Terminal input for setup wizard (already in project) | Already used for tamux-tui |
| `reqwest` | 0.12 | Connectivity test in setup wizard (already in project) | Already used for LLM API calls |

### Supporting
| Library/Tool | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `sha256sum` / `shasum` | System | Checksum verification in install scripts | Always for integrity verification |
| `tar` + `gzip` | System | Archive extraction in install scripts | Standard Unix tools, always available |
| MinGW (`x86_64-w64-mingw32-gcc`) | System | Windows cross-compilation from Linux | Already installed in dev environment |
| `electron-builder` | 25.x (26.8.1 installed) | Electron packaging for desktop apps | Per-platform desktop app archives |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Postinstall download (D-01) | optionalDependencies a la esbuild | optionalDeps requires publishing ~8 separate npm packages per release; postinstall is simpler for a private GitLab-hosted project |
| `cross` for ARM64 | Native cargo with `aarch64-linux-gnu-gcc` | `cross` handles sysroot and linker config automatically via Docker; native requires manual toolchain setup |
| `glab` for releases | `release-cli` | `release-cli` is deprecated as of 2025; `glab` is the official replacement |
| cargo-dist | Custom GitLab CI | cargo-dist generates GitHub Actions primarily; GitLab CI support is limited and less documented; custom CI leverages existing build-release.sh |

**Note on cargo-dist (DIST-02):** The requirement text says "cargo-dist" but the CONTEXT.md decisions (D-05 through D-09) specify custom GitLab CI matrix builds. cargo-dist's primary CI backend is GitHub Actions; its GitLab support is minimal. The implementation uses custom GitLab CI that achieves the same outcome (standalone binaries for all platforms) using the project's existing build scripts. This satisfies the DIST-02 spirit of "standalone platform binaries" without the cargo-dist tool itself.

## Architecture Patterns

### Recommended Project Structure
```
npm-package/                    # New: npm wrapper package (lives at repo root or separate dir)
  package.json                # name: "tamux", bin: { tamux: "bin/tamux" }
  install.js                  # postinstall: detect platform, download binaries
  bin/
    tamux                     # Shell wrapper that finds and exec's the binary
scripts/
  install.sh                  # New: Unix curl-pipe installer
  install.ps1                 # New: PowerShell installer
  build-release.sh            # Existing: core build script
  build-production-releases.sh # Existing: multi-platform orchestrator
  bump-version.sh             # Existing: version sync
.gitlab-ci.yml                # New: CI pipeline config
crates/amux-cli/src/
  setup_wizard.rs             # New: first-run setup wizard module
  main.rs                     # Modified: add setup wizard trigger
```

### Pattern 1: npm Postinstall Binary Download
**What:** A thin npm package that contains only JavaScript glue code. On `npm install` / `npx`, a `postinstall` script runs `install.js` which detects the current OS/arch, constructs the download URL for GitLab Releases, downloads the tarball, extracts binaries, and verifies SHA256 checksums.
**When to use:** Always -- this is the primary distribution channel per D-01.
**Key details:**
- `process.platform` returns: `linux`, `darwin`, `win32`
- `process.arch` returns: `x64`, `arm64`
- Platform map: `{linux-x64, linux-arm64, darwin-arm64, win32-x64}`
- Download URL template: `https://gitlab.com/{org}/tamux/-/releases/v{version}/downloads/tamux-binaries-v{version}-{platform}-{arch}.tar.gz`
- Binaries extracted to `node_modules/tamux/bin/` (or `node_modules/.tamux-bin/`)
- The `bin/tamux` entry in package.json points to a JS wrapper that spawns the native binary

```javascript
// install.js (postinstall script)
const https = require("https");
const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

const VERSION = require("./package.json").version;
const PLATFORM_MAP = {
  "linux-x64": "linux-x64",
  "linux-arm64": "linux-arm64",
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "win32-x64": "windows-x64",
};

const key = `${os.platform()}-${os.arch()}`;
const target = PLATFORM_MAP[key];
if (!target) {
  console.error(`Unsupported platform: ${key}`);
  process.exit(1);
}

const url = `https://gitlab.com/ORG/tamux/-/releases/v${VERSION}/downloads/tamux-binaries-v${VERSION}-${target}.tar.gz`;
// Download, verify SHA256, extract to bin/
```

### Pattern 2: GitLab CI Release Pipeline
**What:** A `.gitlab-ci.yml` with matrix jobs triggered by `vX.Y.Z` tags. Each job builds for one platform target, uploads artifacts to Generic Package Registry, then a final release job creates the GitLab Release with all artifact links.
**When to use:** Every tagged release.
**Key stages:** `build` (matrix: linux-x64, linux-arm64, darwin-arm64, windows-x64) -> `package` (electron per platform) -> `release` (create GitLab Release, publish npm).

```yaml
# .gitlab-ci.yml skeleton
stages:
  - build
  - package
  - release

.rust-build:
  stage: build
  script:
    - ./scripts/build-release.sh --skip-frontend --skip-electron --target $RUST_TARGET
    - tar czf tamux-binaries-v${VERSION}-${PLATFORM}.tar.gz -C dist-release/${PLATFORM_DIR} .
    - 'curl --header "JOB-TOKEN: $CI_JOB_TOKEN" --upload-file tamux-binaries-v${VERSION}-${PLATFORM}.tar.gz
       "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/tamux/${VERSION}/tamux-binaries-v${VERSION}-${PLATFORM}.tar.gz"'
  rules:
    - if: '$CI_COMMIT_TAG =~ /^v\d+\.\d+\.\d+$/'

build:linux-x64:
  extends: .rust-build
  tags: [linux, x64]
  variables:
    RUST_TARGET: x86_64-unknown-linux-gnu
    PLATFORM: linux-x64
    PLATFORM_DIR: linux

build:linux-arm64:
  extends: .rust-build
  tags: [linux, x64]  # cross-compile from x64 using cross
  variables:
    RUST_TARGET: aarch64-unknown-linux-gnu
    PLATFORM: linux-arm64
    PLATFORM_DIR: linux
  before_script:
    - cargo install cross --locked || true
    - cross build --release --target $RUST_TARGET
```

### Pattern 3: First-Run Detection and Setup Wizard
**What:** When `tamux` CLI (or `npx tamux`) runs and `~/.tamux/agent/config.json` does not exist (or has no `provider` set), the setup wizard intercepts before normal operation.
**When to use:** First invocation after install.
**Detection logic:**
1. Check if `~/.tamux/agent/config.json` exists
2. If it exists, check if `provider` field is set and non-empty
3. If either check fails, launch setup wizard
4. After wizard completes, write config and proceed to start daemon + TUI

```rust
// setup_wizard.rs pattern
use crossterm::{event, terminal};
use std::io::Write;

pub fn needs_setup() -> bool {
    let config_path = amux_protocol::tamux_data_dir()
        .join("agent")
        .join("config.json");
    if !config_path.exists() {
        return true;
    }
    match std::fs::read_to_string(&config_path) {
        Ok(contents) => {
            let value: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
            value.get("provider").and_then(|v| v.as_str()).unwrap_or("").is_empty()
        }
        Err(_) => true,
    }
}

pub fn run_setup_wizard() -> anyhow::Result<SetupResult> {
    // Step 1: Provider selection (numbered list of 20+ providers)
    // Step 2: API key entry (masked input)
    // Step 3: Default client (TUI / Electron)
    // Step 4: Data directory (~/.tamux/ or custom)
    // Step 5: Connectivity test (send a minimal completion request)
    // Write config to ~/.tamux/agent/config.json
}
```

### Pattern 4: Install Script (Unix)
**What:** A self-contained shell script downloadable via `curl -fsSL https://tamux.dev/install.sh | sh` that detects platform, downloads binaries from GitLab Releases, verifies checksums, extracts to `~/.local/bin`, and patches shell profiles if needed.
**When to use:** Unix users who want a simple install without Node.js.

Key behaviors:
- Detect OS (`uname -s`) and arch (`uname -m`) -- map to release target names
- Download both tarball and SHA256SUMS file
- Verify checksum before extraction
- Create `~/.local/bin` if it doesn't exist
- Extract binaries (tamux-daemon, tamux, tamux-tui) to `~/.local/bin`
- Check if `~/.local/bin` is in PATH; if not, append to `~/.bashrc`, `~/.zshrc`, `~/.profile`
- Optionally prompt for Electron desktop app download

### Anti-Patterns to Avoid
- **Building npm optionalDependencies packages:** Requires publishing 4-8 separate scoped packages per release. Overkill for a GitLab-hosted project where postinstall download from releases is simpler and equally reliable.
- **Hardcoding version in install scripts:** Version should be determined at CI build time and baked into the script. The install script hosted at `tamux.dev/install.sh` should fetch the latest release version dynamically.
- **Running setup wizard in daemon:** The wizard must run in the CLI/TUI process (interactive terminal), not in the background daemon. The daemon has no terminal.
- **Requiring sudo for Unix installs:** Installing to `~/.local/bin` avoids privilege escalation. The install script must NOT default to `/usr/local/bin`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ARM64 cross-compilation toolchain | Manual linker/sysroot setup | `cross` tool with Docker | Handles 20+ edge cases (libc version, linker flags, sysroot paths) automatically |
| Electron packaging per platform | Custom archive scripts | `electron-builder` 25.x+ | Already configured in `frontend/package.json` build section; handles code signing, auto-update manifests, per-platform formats |
| Version synchronization | Manual edits across files | Existing `bump-version.sh` | Already updates Cargo.toml, package.json, AboutTab.tsx, plugin files, and lockfiles |
| SHA256 checksum generation | Custom hashing script | Existing `checksum_file()` in `build-release.sh` | Already handles sha256sum/shasum/python3 fallback chain |
| Platform detection (bash) | Custom uname parsing | Existing pattern in `build-release.sh` | Already handles Linux/Darwin/MINGW/MSYS/CYGWIN detection |
| Release notes generation | Manual changelog | Existing `generate_release_notes_if_missing()` in `build-release.sh` | Already generates structured release notes with artifact listings |

**Key insight:** The existing build scripts (`build-release.sh`, `build-production-releases.sh`, `setup.sh`, `bump-version.sh`) already implement most of the platform detection, checksum, and packaging logic. The CI pipeline and install scripts should delegate to these scripts rather than reimplementing.

## Common Pitfalls

### Pitfall 1: Postinstall Scripts Disabled
**What goes wrong:** npm, yarn, and pnpm can be configured to skip postinstall scripts. If disabled, binaries are never downloaded and `npx tamux` fails silently or with a confusing error.
**Why it happens:** Security best practices recommend disabling postinstall scripts to prevent supply chain attacks. pnpm shows warnings for packages with postinstall scripts.
**How to avoid:** The `bin/tamux` wrapper script must detect missing binaries at runtime and attempt the download then. Two-layer approach: postinstall tries first (fast path), bin wrapper retries on failure (fallback path).
**Warning signs:** `npx tamux` exits with "binary not found" error.

### Pitfall 2: macOS Universal Binary Complexity
**What goes wrong:** Decision D-06 specifies "darwin-arm64 + darwin-x64 (universal binary)." Building a macOS universal binary requires `lipo` tool which is macOS-only. Cannot be cross-compiled from Linux.
**Why it happens:** Universal binaries (fat binaries) are a macOS-specific concept that merges ARM64 and x64 Mach-O binaries.
**How to avoid:** macOS build must run on a macOS runner (or use a macOS cross-compilation setup). If no macOS CI runner is available, ship separate darwin-arm64 and darwin-x64 tarballs instead of a universal binary. The npm postinstall script already detects arch, so separate binaries work fine.
**Warning signs:** CI pipeline has no macOS runner available.

### Pitfall 3: portable-pty Cross-Compilation Issues
**What goes wrong:** The `portable-pty` crate (used by tamux-daemon) wraps platform-specific PTY APIs and may fail to cross-compile, especially for ARM64 Linux from x86_64.
**Why it happens:** `portable-pty` depends on libc and system headers that must match the target platform.
**How to avoid:** Use `cross` tool which provides Docker containers with the correct target sysroot. Test ARM64 cross-compilation early. If `portable-pty` fails with `cross`, may need a custom `Cross.toml` with pre-build commands.
**Warning signs:** Linker errors mentioning `forkpty`, `openpty`, or `util.h` during ARM64 build.

### Pitfall 4: GitLab Release URL Format
**What goes wrong:** The download URL pattern for GitLab Releases differs from GitHub. GitLab uses the Generic Package Registry URL format, not the release asset download format.
**Why it happens:** GitLab Releases link to package registry artifacts, not direct file downloads like GitHub.
**How to avoid:** Upload artifacts to the Generic Package Registry first, then link them in the release. Download URL format: `https://gitlab.com/api/v4/projects/{PROJECT_ID}/packages/generic/tamux/{VERSION}/{filename}`. The npm postinstall script and install.sh must use this exact format. Alternatively, use release asset links which provide a shorter URL.
**Warning signs:** 404 errors from download URLs during install.

### Pitfall 5: PATH Not Updated After Install
**What goes wrong:** User installs via `install.sh`, but `~/.local/bin` is not in their PATH. Running `tamux` fails with "command not found."
**Why it happens:** Not all Linux distributions add `~/.local/bin` to PATH by default. macOS does not include it. The PATH change requires sourcing the shell profile or opening a new terminal.
**How to avoid:** After patching shell profiles, print a clear message: "Run `source ~/.bashrc` or open a new terminal to use tamux." Also detect the current shell and patch the correct profile file (`.bashrc`, `.zshrc`, `.profile`, `.bash_profile`).
**Warning signs:** User reports "tamux: command not found" immediately after install.

### Pitfall 6: First-Run Config Race Condition
**What goes wrong:** Setup wizard writes `config.json`, then starts daemon. But the daemon may read an empty/default config before the file is fully written.
**Why it happens:** File write is not atomic; daemon may start reading before write completes.
**How to avoid:** Write config to a temp file first, then atomically rename to `config.json`. Start daemon only AFTER rename completes. The daemon's existing `load_config_from_history` reads from SQLite (not the file), so the wizard should write to both the file (for persistence) and send an IPC config update after daemon starts.
**Warning signs:** Daemon starts with default config despite wizard completion.

### Pitfall 7: npm Package Version Mismatch
**What goes wrong:** The npm package version doesn't match the GitLab Release tag version. Postinstall script constructs wrong download URL and gets 404.
**Why it happens:** `bump-version.sh` updates `Cargo.toml` and `frontend/package.json` but doesn't know about the new npm wrapper package.
**How to avoid:** Add the npm wrapper package's `package.json` to `bump-version.sh` so version stays in sync. The CI pipeline should verify version match before publishing.
**Warning signs:** 404 errors during `npm install tamux`.

### Pitfall 8: Windows PATH Update Requires Restart
**What goes wrong:** PowerShell installer updates the system PATH but the change only takes effect in new terminal sessions.
**Why it happens:** Windows environment variable changes via `[Environment]::SetEnvironmentVariable` update the registry but not the current process environment.
**How to avoid:** After updating the registry, also update `$env:PATH` in the current session. Print a message telling the user to open a new terminal for other applications to see the change.
**Warning signs:** User can run `tamux` in the installer's terminal but not in others until restart.

## Code Examples

### npm Package package.json Structure

```json
{
  "name": "tamux",
  "version": "0.1.10",
  "description": "The Agent That Lives - daemon-first AI agent runtime",
  "license": "MIT",
  "homepage": "https://tamux.app",
  "repository": {
    "type": "git",
    "url": "https://gitlab.com/ORG/tamux"
  },
  "bin": {
    "tamux": "bin/tamux.js"
  },
  "scripts": {
    "postinstall": "node install.js"
  },
  "os": ["linux", "darwin", "win32"],
  "cpu": ["x64", "arm64"],
  "engines": {
    "node": ">=18"
  }
}
```
Source: Pattern derived from esbuild npm package structure and Sentry blog post on publishing binaries.

### Postinstall Download with Checksum Verification

```javascript
#!/usr/bin/env node
// install.js -- download platform binaries on npm install
"use strict";

const https = require("https");
const fs = require("fs");
const path = require("path");
const os = require("os");
const { execSync } = require("child_process");
const crypto = require("crypto");

const VERSION = require("./package.json").version;
const BIN_DIR = path.join(__dirname, "bin");

const PLATFORM_MAP = {
  "linux-x64":    "linux-x64",
  "linux-arm64":  "linux-arm64",
  "darwin-arm64": "darwin-arm64",
  "darwin-x64":   "darwin-x64",
  "win32-x64":    "windows-x64",
};

const key = `${os.platform()}-${os.arch()}`;
const target = PLATFORM_MAP[key];

if (!target) {
  console.warn(`tamux: unsupported platform ${key}, skipping binary download`);
  process.exit(0); // Don't fail install -- user might be in CI
}

const BASE_URL = `https://gitlab.com/api/v4/projects/PROJECT_ID/packages/generic/tamux/${VERSION}`;
const TARBALL = `tamux-binaries-v${VERSION}-${target}.tar.gz`;
const CHECKSUMS = `SHA256SUMS-${target}.txt`;

async function download(url, dest) {
  // Follow redirects, write to file
}

async function verifyChecksum(file, expectedHash) {
  const hash = crypto.createHash("sha256");
  const stream = fs.createReadStream(file);
  for await (const chunk of stream) hash.update(chunk);
  return hash.digest("hex") === expectedHash;
}

async function main() {
  fs.mkdirSync(BIN_DIR, { recursive: true });
  // 1. Download tarball and checksums
  // 2. Verify SHA256
  // 3. Extract (tar xzf on Unix, node tar on Windows)
  // 4. Set executable permissions (chmod +x)
}

main().catch((err) => {
  console.warn(`tamux: postinstall binary download failed: ${err.message}`);
  console.warn("tamux: binaries will be downloaded on first run");
  process.exit(0); // Don't block npm install
});
```
Source: Pattern from Sentry engineering blog and esbuild install.js approach.

### Unix Install Script Pattern

```bash
#!/bin/sh
# install.sh -- install tamux binaries to ~/.local/bin
set -e

INSTALL_DIR="${TAMUX_INSTALL_DIR:-$HOME/.local/bin}"
BASE_URL="https://gitlab.com/api/v4/projects/PROJECT_ID/packages/generic/tamux"

detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"
  case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="darwin" ;;
    *)      echo "Unsupported OS: $OS" >&2; exit 1 ;;
  esac
  case "$ARCH" in
    x86_64|amd64) ARCH="x64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)             echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
  esac
  TARGET="${PLATFORM}-${ARCH}"
}

get_latest_version() {
  # Fetch latest tag from GitLab API or use pinned version
  VERSION="${TAMUX_VERSION:-latest}"
}

download_and_verify() {
  TARBALL="tamux-binaries-v${VERSION}-${TARGET}.tar.gz"
  SUMS="SHA256SUMS-${TARGET}.txt"

  echo "Downloading tamux v${VERSION} for ${TARGET}..."
  curl -fsSL "${BASE_URL}/${VERSION}/${TARBALL}" -o "/tmp/${TARBALL}"
  curl -fsSL "${BASE_URL}/${VERSION}/${SUMS}" -o "/tmp/${SUMS}"

  # Verify checksum
  if command -v sha256sum >/dev/null 2>&1; then
    (cd /tmp && sha256sum -c "${SUMS}")
  elif command -v shasum >/dev/null 2>&1; then
    (cd /tmp && shasum -a 256 -c "${SUMS}")
  else
    echo "Warning: no sha256sum found, skipping verification"
  fi
}

install_binaries() {
  mkdir -p "$INSTALL_DIR"
  tar xzf "/tmp/${TARBALL}" -C "$INSTALL_DIR"
  chmod +x "$INSTALL_DIR/tamux-daemon" "$INSTALL_DIR/tamux" "$INSTALL_DIR/tamux-tui"
  echo "Installed: tamux-daemon, tamux, tamux-tui -> $INSTALL_DIR"
}

ensure_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) return ;; # Already in PATH
  esac

  for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    if [ -f "$profile" ]; then
      echo "" >> "$profile"
      echo "# Added by tamux installer" >> "$profile"
      echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$profile"
    fi
  done
  echo ""
  echo "Added $INSTALL_DIR to PATH in shell profiles."
  echo "Run 'source ~/.bashrc' (or open a new terminal) to start using tamux."
}

detect_platform
get_latest_version
download_and_verify
install_binaries
ensure_path
echo ""
echo "tamux installed successfully! Run 'tamux' to get started."
```
Source: Pattern from rustup installer, uv installer, and XDG Base Directory conventions.

### First-Run Detection in CLI

```rust
// In crates/amux-cli/src/main.rs, before normal command dispatch:
fn is_first_run() -> bool {
    let config_dir = amux_protocol::tamux_data_dir().join("agent");
    let config_path = config_dir.join("config.json");
    if !config_path.exists() {
        return true;
    }
    // Check if provider is configured
    match std::fs::read_to_string(&config_path) {
        Ok(contents) => {
            let value: serde_json::Value = serde_json::from_str(&contents).unwrap_or_default();
            let provider = value.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            provider.is_empty()
        }
        Err(_) => true,
    }
}
```
Source: Follows existing config loading pattern in `crates/amux-daemon/src/agent/config.rs` and `crates/amux-protocol/src/config.rs`.

### GitLab CI Release Job

```yaml
release:
  stage: release
  image: registry.gitlab.com/gitlab-org/cli:latest
  script:
    - VERSION="${CI_COMMIT_TAG#v}"
    - |
      glab release create "$CI_COMMIT_TAG" \
        --name "tamux ${VERSION}" \
        --notes "Release ${VERSION}" \
        --assets-links '[
          {"name": "tamux-binaries-linux-x64", "url": "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/tamux/${VERSION}/tamux-binaries-v${VERSION}-linux-x64.tar.gz"},
          {"name": "tamux-binaries-linux-arm64", "url": "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/tamux/${VERSION}/tamux-binaries-v${VERSION}-linux-arm64.tar.gz"},
          {"name": "SHA256SUMS", "url": "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/tamux/${VERSION}/SHA256SUMS.txt"}
        ]'
  rules:
    - if: '$CI_COMMIT_TAG =~ /^v\d+\.\d+\.\d+$/'
```
Source: GitLab Releases documentation, glab CLI documentation.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `release-cli` for GitLab CI releases | `glab` CLI (v1.58.0+) | 2025 | Must use `glab` in CI; `release-cli` deprecated |
| npm postinstall only | optionalDeps + postinstall fallback | 2021 (esbuild PR #1621) | pnpm warns on postinstall scripts; may need bin-wrapper fallback |
| cargo-dist for all CI | cargo-dist GitHub-first, custom for GitLab | Ongoing | cargo-dist GitLab CI support is minimal; custom CI is more reliable for GitLab projects |
| `/usr/local/bin` install target | `~/.local/bin` (XDG) | ~2020 | Modern distros and tools (rustup, uv) prefer user-local installs |

**Deprecated/outdated:**
- `release-cli`: Deprecated in favor of `glab` CLI; container images updated to include `glab`
- npm `node-pre-gyp`: Replaced by `prebuild-install` / `prebuildify` for native addons; not needed here since tamux binaries are standalone (not Node.js native addons)

## Open Questions

1. **GitLab CI Runner Availability**
   - What we know: The project is hosted on GitLab. Linux x64 runners should be available (shared or project runners). Windows and macOS runners may require self-hosted runners or GitLab Premium/Ultimate.
   - What's unclear: Whether the GitLab instance has macOS and Windows runners, or whether cross-compilation from Linux is needed for all targets.
   - Recommendation: Plan for Linux-only CI runners. Use `cross` for ARM64, `x86_64-w64-mingw32-gcc` (MinGW) for Windows cross-compilation (already set up in the project). macOS can be a manual or self-hosted runner step, or deferred to separate darwin-arm64/darwin-x64 targets without universal binary.

2. **npm Registry Scope**
   - What we know: The package name is `tamux` (per D-01). Publishing to public npm requires the name to be available.
   - What's unclear: Whether `tamux` is already claimed on npm. If claimed, may need `@tamux/cli` or similar scoped name.
   - Recommendation: Check `npm view tamux` early. If taken, use `@tamux/tamux` scoped package.

3. **GitLab Project ID for API URLs**
   - What we know: Generic Package Registry URLs require the numeric project ID. CI provides `$CI_PROJECT_ID`.
   - What's unclear: For install scripts running outside CI (user machines), the project ID must be hardcoded or the URL must use a different format.
   - Recommendation: Use the full project path format: `https://gitlab.com/api/v4/projects/{url-encoded-path}/packages/generic/tamux/...` or publish download links in the GitLab Release that use redirect-friendly URLs.

4. **Setup Wizard Terminal Framework**
   - What we know: The TUI already uses ratatui + crossterm. The CLI currently has no interactive input.
   - What's unclear: Whether the setup wizard should use full ratatui (rich TUI) or simpler stdin/stdout prompts (wider compatibility, works in non-TTY contexts).
   - Recommendation: Use simple `crossterm` terminal input (not full ratatui alternate screen) for maximum compatibility. Numbered menu selections, masked password input via crossterm's `DisableLineWrap` + manual echo. This keeps the wizard lightweight and avoids importing the full ratatui rendering stack into the CLI crate.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust stable | Binary compilation | Yes | 1.94.0 | -- |
| Node.js | npm package, Electron | Yes | v24.13.1 | -- |
| npm | Package publishing | Yes | 11.8.0 | -- |
| Docker | `cross` ARM64 builds | Yes | 29.2.1 | MinGW for Windows only |
| MinGW (`x86_64-w64-mingw32-gcc`) | Windows cross-compilation | Yes | Installed | -- |
| `cross` | ARM64 Linux cross-compilation | No | -- | `cargo install cross --locked` in CI |
| `sha256sum` | Checksum verification | Yes | Installed | shasum, python3 hashlib |
| electron-builder | Desktop app packaging | Yes | 26.8.1 | -- |
| `glab` CLI | GitLab release creation | No (CI only) | -- | Container image `registry.gitlab.com/gitlab-org/cli:latest` |
| macOS runner | macOS binary builds | Unknown | -- | Cross-compile or separate manual build |
| `aarch64-unknown-linux-gnu` Rust target | ARM64 compilation | No (needs `rustup target add`) | -- | `cross` handles this automatically |

**Missing dependencies with no fallback:**
- None blocking -- all critical tools are available or installable in CI

**Missing dependencies with fallback:**
- `cross`: Install via `cargo install cross --locked` in CI pipeline
- `glab` CLI: Use official container image in CI
- `aarch64-unknown-linux-gnu` target: `cross` handles without explicit target add

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + cargo test |
| Config file | Standard Cargo test infrastructure (no custom config) |
| Quick run command | `cargo test -p tamux-cli -- --test-threads=1 -q` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DIST-01 | npm postinstall detects platform and constructs correct URL | unit (JS) | `node npm-package/install.js --dry-run` (add dry-run flag) | No -- Wave 0 |
| DIST-01 | bin wrapper finds and spawns correct binary | unit (JS) | `node npm-package/bin/tamux.js --version` | No -- Wave 0 |
| DIST-02 | Build script produces binaries for target platform | integration | `./scripts/build-release.sh --skip-frontend --skip-electron && test -f dist-release/linux/tamux-daemon` | Partially (existing script) |
| DIST-03 | Install script detects platform correctly | unit (shell) | Inline shell function tests in install.sh | No -- Wave 0 |
| DIST-04 | PowerShell installer detects arch and downloads | unit (PS) | `pwsh -File scripts/install.ps1 -DryRun` | No -- Wave 0 |
| DIST-05 | First-run detection returns true when no config | unit (Rust) | `cargo test -p tamux-cli -- setup_wizard` | No -- Wave 0 |
| DIST-05 | First-run detection returns false when config exists | unit (Rust) | `cargo test -p tamux-cli -- setup_wizard` | No -- Wave 0 |
| DIST-05 | Setup wizard writes valid config JSON | unit (Rust) | `cargo test -p tamux-cli -- setup_wizard` | No -- Wave 0 |
| DIST-05 | Connectivity test validates provider URL | unit (Rust) | `cargo test -p tamux-cli -- connectivity` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p tamux-cli -q`
- **Per wave merge:** `cargo test --workspace -q`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/amux-cli/src/setup_wizard.rs` -- unit tests for first-run detection, config writing, provider list
- [ ] `npm-package/test/install.test.js` -- unit tests for platform detection, URL construction, checksum verification (Node.js)
- [ ] Shell test functions inside `scripts/install.sh` for platform detection (invoked with `--test` flag)
- [ ] Framework install: `cargo install cross --locked` -- needed for ARM64 cross-compilation testing

## Sources

### Primary (HIGH confidence)
- Existing project codebase: `scripts/build-release.sh`, `scripts/build-production-releases.sh`, `scripts/bump-version.sh`, `scripts/setup.sh` -- build infrastructure
- Existing project codebase: `crates/amux-protocol/src/config.rs` -- data directory paths, config loading
- Existing project codebase: `crates/amux-cli/src/main.rs` -- CLI entry point, `StartDaemon` command
- Existing project codebase: `crates/amux-daemon/src/agent/config.rs` -- agent config structure
- Existing project codebase: `frontend/package.json` build section -- electron-builder config
- Existing project codebase: `Cargo.toml` -- workspace version 0.1.10, 6 member crates
- [GitLab Releases documentation](https://docs.gitlab.com/user/project/releases/) -- release creation, asset links
- [GitLab Generic Package Registry](https://docs.gitlab.com/user/packages/generic_packages/) -- artifact upload via CI job token

### Secondary (MEDIUM confidence)
- [esbuild optionalDependencies PR #1621](https://github.com/evanw/esbuild/pull/1621) -- platform-specific binary pattern
- [Sentry: Publishing Binaries on npm](https://sentry.engineering/blog/publishing-binaries-on-npm) -- combined optionalDeps + postinstall pattern
- [DeepWiki: esbuild Platform-Specific Binaries](https://deepwiki.com/evanw/esbuild/6.2-platform-specific-binaries) -- detailed package structure
- [cross-rs/cross](https://github.com/cross-rs/cross) -- ARM64 cross-compilation from x64 Linux
- [Mike Ensor: Release multi-target Rust applications with GitLab CI](https://mike-ensor.medium.com/release-multi-target-rust-applications-with-gitlab-ci-90136fa10e4c) -- GitLab CI matrix build pattern
- [cargo-dist](https://axodotdev.github.io/cargo-dist/) -- primarily GitHub Actions; minimal GitLab support confirmed

### Tertiary (LOW confidence)
- macOS CI runner availability for the project's GitLab instance -- unverified
- npm package name `tamux` availability -- needs manual `npm view tamux` check
- `portable-pty` cross-compilation to ARM64 -- untested in this project, may surface linker issues

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- using proven npm patterns, existing project build scripts, well-documented GitLab CI features
- Architecture: HIGH -- patterns directly derived from esbuild/Sentry/rustup models with project-specific adaptations
- Pitfalls: HIGH -- drawn from documented issues in esbuild, GitLab CI, and first-hand cross-compilation experience
- CI pipeline: MEDIUM -- depends on runner availability which is unverified for macOS/Windows targets
- Setup wizard: MEDIUM -- terminal input approach recommendation is sound but needs implementation validation

**Research date:** 2026-03-24
**Valid until:** 2026-04-24 (stable domain -- npm, GitLab CI, and Rust tooling change slowly)
