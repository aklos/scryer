#!/usr/bin/env bash
# Close the WebDriver session (undoing wd.py's localStorage additions) and
# stop everything start.sh began, by pid file.
RUN_DIR=${RUN_DIR:-/tmp/scryer-headless}
python3 "$(dirname "$0")/wd.py" stop 2>/dev/null || true
for name in driver vite xvfb; do
  f="$RUN_DIR/$name.pid"
  [ -f "$f" ] && { pkill -P "$(cat "$f")" 2>/dev/null; kill "$(cat "$f")" 2>/dev/null; rm -f "$f"; }
done
echo stopped
