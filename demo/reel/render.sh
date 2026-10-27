#!/usr/bin/env bash
# Render the reel inside a memory-capped scope, so running out of memory
# kills the render, not the machine. Refuses while a cargo build runs.
# usage: [COMP=id] demo/reel/render.sh [out.mp4] [extra remotion args…]
set -euo pipefail
cd "$(dirname "$0")/../.."
if pgrep -x rustc >/dev/null || pgrep -x cargo >/dev/null; then
  echo "render.sh: a cargo build is running; render after it finishes." >&2
  exit 1
fi
unset WAYLAND_DISPLAY
out="${1:-demo/out/reel.mp4}"
shift || true
exec systemd-run --user --scope --quiet -p MemoryMax=8G -p MemorySwapMax=0 \
  npx remotion render demo/reel/index.ts "${COMP:-reel}" "$out" --codec=h264 \
  --config=demo/reel/remotion.config.ts "$@"
