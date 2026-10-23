---
name: run-app-headless
description: Launch the scryer desktop app (Tauri) on a private Xvfb display and drive its webview over WebDriver (tauri-driver) — open a project, click, read the DOM, take screenshots. Use to validate a UI change in the real app without touching the developer's screen.
---

# Run the app headless

The app is a Tauri window (WebKitGTK). It is driven through `tauri-driver`,
which speaks WebDriver and hands the webview to `WebKitWebDriver`, on its own
Xvfb display (`:99`), so nothing appears on the developer's desktop.

## One command

```bash
.claude/skills/run-app-headless/scripts/start.sh            # build, Xvfb, vite, tauri-driver, app
python3 .claude/skills/run-app-headless/scripts/wd.py open "$PWD"   # open this repo as the project
python3 .claude/skills/run-app-headless/scripts/wd.py shot /tmp/app.png
.claude/skills/run-app-headless/scripts/stop.sh
```

`start.sh` is idempotent: it builds the debug `scryer-mcp` sidecar
(`cargo run -p xtask -- build-sidecar --debug`) and the app binary if
needed, links `node_modules` from the main checkout when run in a worktree,
starts Xvfb on `:99`, Vite on port 1420 (`--strictPort`), and tauri-driver on
4444, then opens a WebDriver session on `target/debug/scryer`. Logs and pid
files go to `$RUN_DIR` (default `/tmp/scryer-headless`).

## Driving

`wd.py` is a dependency-free WebDriver client:

| command | does |
|---|---|
| `wd.py open <project>` | puts the project first in Recent and clicks it; waits for the workspace |
| `wd.py js '<script>'` | runs a script in the webview (`return …` to get a value back) |
| `wd.py shot <png>` | screenshot of the window — **read it**, a blank frame is a failure |
| `wd.py click '<text>'` | clicks the first button, link or menu row whose text is exactly that |
| `wd.py resize <w> <h>` | resizes the window (default after start: 1500×950) |

Recipes that worked:
- Typing into the in-place editors (contenteditable): focus the element,
  select its contents, then `document.execCommand('insertText', false, text)`
  so React sees the input.
- A tree row's context menu: dispatch a `contextmenu` MouseEvent with
  `bubbles: true` and client coordinates on the row's label span; menu rows
  are `<li>`s, click them by text.
- Section editors expose their toggle as the first `button` inside
  `section[data-section="Claims"]`; their Done button is matched
  by text.

## Watch out

- **Never on the developer's display.** start.sh unsets `WAYLAND_DISPLAY`
  and exports `DISPLAY=:99` and `GDK_BACKEND=x11` for everything it starts,
  and refuses to run if the environment still reaches the desktop session,
  if Xvfb `:99` is not up, or if a running tauri-driver can see the session.
  With `WAYLAND_DISPLAY` inherited, GTK picks Wayland over `DISPLAY` and the
  window opens on the real desktop. Never launch the app, `tauri dev`,
  tauri-driver or a browser any other way.

- The webview uses the developer's real app profile: `localStorage`
  (recent projects, per-project settings) is shared with their app. Remove
  whatever you add when done — `wd.py open` records what it added and
  `stop.sh` takes it back out.
- Don't click "Enable" on the AI-integration prompt: it writes `.mcp.json`
  and `.claude/settings.local.json` into the project. Click "Not now".
- Never `pkill -f` a pattern that also appears in your own command line; it
  kills your shell. `stop.sh` kills by pid file.
