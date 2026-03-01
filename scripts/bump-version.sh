#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.1.4"
  exit 1
fi

VERSION="$1"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# All Cargo.toml files with a version field to bump
CARGO_FILES=(
  "$ROOT/src-tauri/Cargo.toml"
  "$ROOT/crates/scryer-core/Cargo.toml"
  "$ROOT/crates/scryer-mcp/Cargo.toml"
  "$ROOT/crates/scryer-suggest/Cargo.toml"
  "$ROOT/crates/xtask/Cargo.toml"
)

for f in "${CARGO_FILES[@]}"; do
  sed -i "0,/^version = \".*\"/s//version = \"$VERSION\"/" "$f"
  echo "  updated $f"
done

# package.json
sed -i "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" "$ROOT/package.json"
echo "  updated package.json"

# tauri.conf.json
sed -i "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" "$ROOT/src-tauri/tauri.conf.json"
echo "  updated tauri.conf.json"

# Regenerate Cargo.lock
(cd "$ROOT" && cargo generate-lockfile 2>/dev/null)
echo "  updated Cargo.lock"

echo ""
echo "Bumped to v$VERSION. Now run:"
echo "  git add -A && git commit -m 'Bump version to $VERSION'"
echo "  git tag v$VERSION"
echo "  git push && git push --tags"
