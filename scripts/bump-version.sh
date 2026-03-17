#!/usr/bin/env bash
#
# bump-version.sh — Update the tamux version across the entire stack.
#
# Usage:
#   ./scripts/bump-version.sh 0.2.0
#   ./scripts/bump-version.sh patch    # 0.1.7 → 0.1.8
#   ./scripts/bump-version.sh minor    # 0.1.7 → 0.2.0
#   ./scripts/bump-version.sh major    # 0.1.7 → 1.0.0
#
# Files updated:
#   - Cargo.toml                  (workspace version)
#   - frontend/package.json       (npm version + triggers lock update)
#   - frontend/src/components/settings-panel/AboutTab.tsx
#   - frontend/src/plugins/coding-agents/registerPlugin.ts
#   - frontend/src/plugins/ai-training/registerPlugin.ts
#   - docs/plugin-development.md
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ---------------------------------------------------------------------------
# Resolve current version from Cargo.toml (single source of truth)
# ---------------------------------------------------------------------------
CURRENT=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
if [[ -z "$CURRENT" ]]; then
  echo "Error: could not read current version from Cargo.toml" >&2
  exit 1
fi

IFS='.' read -r CUR_MAJOR CUR_MINOR CUR_PATCH <<< "$CURRENT"

# ---------------------------------------------------------------------------
# Determine next version
# ---------------------------------------------------------------------------
ARG="${1:-}"
if [[ -z "$ARG" ]]; then
  echo "Current version: $CURRENT"
  echo ""
  echo "Usage: $0 <version|patch|minor|major>"
  echo "  $0 patch  →  $CUR_MAJOR.$CUR_MINOR.$((CUR_PATCH + 1))"
  echo "  $0 minor  →  $CUR_MAJOR.$((CUR_MINOR + 1)).0"
  echo "  $0 major  →  $((CUR_MAJOR + 1)).0.0"
  echo "  $0 X.Y.Z  →  X.Y.Z"
  exit 0
fi

case "$ARG" in
  patch) NEXT="$CUR_MAJOR.$CUR_MINOR.$((CUR_PATCH + 1))" ;;
  minor) NEXT="$CUR_MAJOR.$((CUR_MINOR + 1)).0" ;;
  major) NEXT="$((CUR_MAJOR + 1)).0.0" ;;
  *)
    if [[ ! "$ARG" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "Error: invalid version '$ARG'. Use semver (X.Y.Z) or patch/minor/major." >&2
      exit 1
    fi
    NEXT="$ARG"
    ;;
esac

if [[ "$CURRENT" == "$NEXT" ]]; then
  echo "Already at version $CURRENT — nothing to do."
  exit 0
fi

echo "Bumping version: $CURRENT → $NEXT"
echo ""

# ---------------------------------------------------------------------------
# Helper: replace version in a file (with reporting)
# ---------------------------------------------------------------------------
bump_file() {
  local file="$1"
  local pattern="$2"
  local replacement="$3"

  if [[ ! -f "$file" ]]; then
    echo "  SKIP  $file (not found)"
    return
  fi

  if grep -qF "$pattern" "$file"; then
    # Use perl for portable in-place edit (works on both GNU and BSD/macOS)
    perl -pi -e "s/\Q${pattern}\E/${replacement}/g" "$file"
    echo "  OK    $file"
  else
    echo "  SKIP  $file (pattern not found)"
  fi
}

# ---------------------------------------------------------------------------
# Rust workspace
# ---------------------------------------------------------------------------
bump_file "Cargo.toml" \
  "version = \"$CURRENT\"" \
  "version = \"$NEXT\""

# ---------------------------------------------------------------------------
# Frontend package.json (version field only — lock updates via npm)
# ---------------------------------------------------------------------------
bump_file "frontend/package.json" \
  "\"version\": \"$CURRENT\"" \
  "\"version\": \"$NEXT\""

# ---------------------------------------------------------------------------
# AboutTab.tsx — displayed version string
# ---------------------------------------------------------------------------
bump_file "frontend/src/components/settings-panel/AboutTab.tsx" \
  "Version $CURRENT" \
  "Version $NEXT"

# ---------------------------------------------------------------------------
# Built-in plugins
# ---------------------------------------------------------------------------
bump_file "frontend/src/plugins/coding-agents/registerPlugin.ts" \
  "version: \"$CURRENT\"" \
  "version: \"$NEXT\""

bump_file "frontend/src/plugins/ai-training/registerPlugin.ts" \
  "version: \"$CURRENT\"" \
  "version: \"$NEXT\""

# ---------------------------------------------------------------------------
# Documentation
# ---------------------------------------------------------------------------
bump_file "docs/plugin-development.md" \
  "\"version\": \"$CURRENT\"" \
  "\"version\": \"$NEXT\""

bump_file "docs/plugin-development.md" \
  "version: \"$CURRENT\"" \
  "version: \"$NEXT\""

# ---------------------------------------------------------------------------
# Update package-lock.json (npm updates the lock automatically)
# ---------------------------------------------------------------------------
if [[ -f "frontend/package-lock.json" ]]; then
  echo ""
  echo "Updating frontend/package-lock.json..."
  (cd frontend && npm install --package-lock-only --ignore-scripts 2>/dev/null) && echo "  OK    frontend/package-lock.json" || echo "  WARN  npm lock update failed — run 'npm install' manually"
fi

# ---------------------------------------------------------------------------
# Update Cargo.lock
# ---------------------------------------------------------------------------
if command -v cargo &>/dev/null; then
  echo "Updating Cargo.lock..."
  # Use cargo check to refresh the lockfile without upgrading dependencies.
  cargo check --workspace 2>/dev/null && echo "  OK    Cargo.lock" || echo "  WARN  cargo check failed — run 'cargo check' manually"
fi

echo ""
echo "Done. Version is now $NEXT across the stack."
echo ""
echo "Files to commit:"
git diff --name-only
