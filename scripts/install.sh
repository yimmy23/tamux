#!/bin/sh
# install.sh -- Install tamux binaries to ~/.local/bin
# Usage: curl -fsSL https://tamux.dev/install.sh | sh
#        TAMUX_VERSION=0.1.10 ./install.sh
#        TAMUX_INSTALL_DIR=/opt/tamux ./install.sh
#        ./install.sh --dry-run
set -e

INSTALL_DIR="${TAMUX_INSTALL_DIR:-$HOME/.local/bin}"
BASE_URL="https://gitlab.com/api/v4/projects/PROJECT_ID/packages/generic/tamux"
BINARIES="tamux-daemon tamux tamux-tui"

# Argument parsing
DRY_RUN=false
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
  esac
done

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)  OS_NAME="linux" ;;
    Darwin) OS_NAME="darwin" ;;
    *)      echo "Error: Unsupported OS: $OS" >&2; exit 1 ;;
  esac

  case "$ARCH" in
    x86_64|amd64)   ARCH_NAME="x64" ;;
    aarch64|arm64)   ARCH_NAME="arm64" ;;
    *)               echo "Error: Unsupported architecture: $ARCH" >&2; exit 1 ;;
  esac

  TARGET="${OS_NAME}-${ARCH_NAME}"
  echo "Detected platform: ${TARGET}"
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

get_latest_version() {
  if [ -n "${TAMUX_VERSION:-}" ]; then
    VERSION="$TAMUX_VERSION"
    echo "Using specified version: ${VERSION}"
    return
  fi

  VERSION="$(curl -fsSL "https://gitlab.com/api/v4/projects/PROJECT_ID/releases" 2>/dev/null \
    | grep -o '"tag_name":"v[^"]*"' \
    | head -1 \
    | grep -o '[0-9][0-9.]*')" || true

  if [ -z "$VERSION" ]; then
    echo "Error: Could not determine latest version. Set TAMUX_VERSION=x.y.z" >&2
    exit 1
  fi

  echo "Latest version: ${VERSION}"
}

# ---------------------------------------------------------------------------
# Download and verify
# ---------------------------------------------------------------------------

download_and_verify() {
  TARBALL="tamux-binaries-v${VERSION}-${TARGET}.tar.gz"
  SUMS="SHA256SUMS-${TARGET}.txt"
  TMP_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_DIR" 2>/dev/null' EXIT

  echo "Downloading tamux v${VERSION} for ${TARGET}..."
  curl -fsSL "${BASE_URL}/${VERSION}/${TARBALL}" -o "${TMP_DIR}/${TARBALL}"
  curl -fsSL "${BASE_URL}/${VERSION}/${SUMS}" -o "${TMP_DIR}/${SUMS}"

  # SHA256 checksum verification
  if command -v sha256sum >/dev/null 2>&1; then
    echo "Verifying SHA256 checksum..."
    (cd "$TMP_DIR" && sha256sum -c "$SUMS")
  elif command -v shasum >/dev/null 2>&1; then
    echo "Verifying SHA256 checksum (shasum)..."
    (cd "$TMP_DIR" && shasum -a 256 -c "$SUMS")
  else
    echo "Warning: sha256sum not found, skipping checksum verification"
  fi
}

# ---------------------------------------------------------------------------
# Install binaries
# ---------------------------------------------------------------------------

install_binaries() {
  mkdir -p "$INSTALL_DIR"
  tar xzf "${TMP_DIR}/${TARBALL}" -C "$INSTALL_DIR"
  chmod +x "$INSTALL_DIR/tamux-daemon" "$INSTALL_DIR/tamux" "$INSTALL_DIR/tamux-tui"
  echo "Installed: tamux-daemon, tamux, tamux-tui -> $INSTALL_DIR"
}

# ---------------------------------------------------------------------------
# Optional desktop app
# ---------------------------------------------------------------------------

offer_desktop_app() {
  # Only prompt when stdin is a terminal (not piped)
  if [ ! -t 0 ]; then
    return
  fi

  printf "Install desktop app? [y/N] "
  read -r answer
  case "$answer" in
    y|Y)
      case "$OS_NAME" in
        linux)
          DESKTOP_EXT="AppImage"
          DESKTOP_ARTIFACT="tamux-desktop-v${VERSION}-${TARGET}.${DESKTOP_EXT}"
          echo "Downloading desktop app..."
          curl -fsSL "${BASE_URL}/${VERSION}/${DESKTOP_ARTIFACT}" -o "$INSTALL_DIR/tamux-desktop.AppImage"
          chmod +x "$INSTALL_DIR/tamux-desktop.AppImage"
          echo "Installed: tamux-desktop.AppImage -> $INSTALL_DIR"
          ;;
        darwin)
          DESKTOP_EXT="dmg"
          DESKTOP_ARTIFACT="tamux-desktop-v${VERSION}-${TARGET}.${DESKTOP_EXT}"
          echo "Downloading desktop app..."
          curl -fsSL "${BASE_URL}/${VERSION}/${DESKTOP_ARTIFACT}" -o "/tmp/tamux-desktop.dmg"
          echo "Open /tmp/tamux-desktop.dmg to install"
          ;;
      esac
      ;;
    *)
      ;;
  esac
}

# ---------------------------------------------------------------------------
# PATH configuration
# ---------------------------------------------------------------------------

ensure_path() {
  # Check if INSTALL_DIR is already in PATH
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) return ;;
  esac

  PATCHED=false
  for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile" "$HOME/.bash_profile"; do
    if [ -f "$profile" ]; then
      echo "" >> "$profile"
      echo "# Added by tamux installer" >> "$profile"
      echo 'export PATH="'"$INSTALL_DIR"':$PATH"' >> "$profile"
      PATCHED=true
    fi
  done

  if [ "$PATCHED" = true ]; then
    echo "Added $INSTALL_DIR to PATH in shell profiles."
    echo "Run 'source ~/.bashrc' or open a new terminal to start using tamux."
  else
    echo "Warning: Could not find shell profile to update PATH."
    echo "Add the following to your shell profile:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

detect_platform
get_latest_version

# --dry-run: print detection results and exit without downloading
if [ "$DRY_RUN" = true ]; then
  TARBALL="tamux-binaries-v${VERSION}-${TARGET}.tar.gz"
  echo ""
  echo "Platform: ${TARGET}"
  echo "Version: ${VERSION}"
  echo "Download URL: ${BASE_URL}/${VERSION}/${TARBALL}"
  echo "Install directory: ${INSTALL_DIR}"
  echo "Dry run complete -- no files downloaded or modified."
  exit 0
fi

download_and_verify
install_binaries
offer_desktop_app
ensure_path

echo ""
echo "tamux installed successfully! Run 'tamux' to get started."
