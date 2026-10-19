#!/usr/bin/env bash
# Start the scryer app headless: Xvfb :99, Vite :1420, tauri-driver :4444,
# and a WebDriver session on the debug app binary. Idempotent.
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
RUN_DIR=${RUN_DIR:-/tmp/scryer-headless}
mkdir -p "$RUN_DIR"
cd "$ROOT"

running() { [ -f "$RUN_DIR/$1.pid" ] && kill -0 "$(cat "$RUN_DIR/$1.pid")" 2>/dev/null; }

# A worktree has no node_modules of its own: borrow the main checkout's.
if [ ! -e node_modules ]; then
  main=$(git worktree list --porcelain | awk '/^worktree /{print $2; exit}')
  [ -d "$main/node_modules" ] && ln -s "$main/node_modules" node_modules
fi

triple=$(rustc -vV | awk '/^host:/{print $2}')
[ -x "src-tauri/binaries/scryer-mcp-$triple" ] || cargo run -q -p xtask -- build-sidecar --debug
(cd src-tauri && cargo build -q)
command -v tauri-driver >/dev/null || [ -x "$HOME/.cargo/bin/tauri-driver" ] || cargo install tauri-driver --locked
DRIVER=$(command -v tauri-driver || echo "$HOME/.cargo/bin/tauri-driver")

if ! running xvfb; then
  Xvfb :99 -screen 0 1600x1000x24 >"$RUN_DIR/xvfb.log" 2>&1 & echo $! >"$RUN_DIR/xvfb.pid"
fi
if ! running vite; then
  npx vite --port 1420 --strictPort >"$RUN_DIR/vite.log" 2>&1 & echo $! >"$RUN_DIR/vite.pid"
fi
if ! running driver; then
  DISPLAY=:99 "$DRIVER" --native-driver "$(command -v WebKitWebDriver)" >"$RUN_DIR/driver.log" 2>&1 &
  echo $! >"$RUN_DIR/driver.pid"
fi
for _ in $(seq 30); do
  (exec 3<>/dev/tcp/127.0.0.1/4444) 2>/dev/null && (exec 3<>/dev/tcp/127.0.0.1/1420) 2>/dev/null && break
  sleep 0.5
done
python3 "$(dirname "$0")/wd.py" start "$ROOT/target/debug/scryer"
python3 "$(dirname "$0")/wd.py" resize 1500 950
echo "app running headless on :99 (logs in $RUN_DIR)"
