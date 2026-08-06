/**
 * Demo-only Tauri shim.
 *
 * The trailer harness lifts real app components and feeds them fixtures. A few
 * of those components (and `theme.ts`) import `@tauri-apps/api/*` to talk to the
 * Rust backend, which doesn't exist when we render in a plain browser. The demo
 * Vite config (`demo/vite.config.ts`) aliases `@tauri-apps/api/core` and
 * `@tauri-apps/api/window` to THIS file so those imports resolve to harmless
 * no-ops. The app's own build never sees this alias — the demo stays
 * non-invasive.
 */

import { SOURCE_SPANS } from "./fixtures";

/**
 * Backend command bridge. No-op for everything, EXCEPT the handful of read
 * commands a scene needs to render real product behaviour without a backend:
 *  - `read_source_span` returns a curated tokenized code span (the inline peek)
 *    keyed by the requested file, so the claim→code reveal shows actual code.
 * Scenes feed everything else through props/fixtures.
 */
export async function invoke<T = unknown>(cmd: string, args?: unknown): Promise<T> {
  if (cmd === "read_source_span") {
    const file = (args as { file?: string } | undefined)?.file ?? "";
    const span = SOURCE_SPANS[file];
    if (span) return span as T;
  }
  // The launch readout (powerline, confirm gate, picker) resolves from these two
  // reads. Serve a real Claude Code · opus · high setup so the lifted ProjectPicker
  // and Powerline show an authentic launch — and `confirmLaunch: false` so the
  // prologue's "Generate" fires straight into the build with no modal.
  if (cmd === "detect_ai_tools") return { claude: true, codex: false, copilot: false } as T;
  if (cmd === "get_subagent_settings") {
    return {
      agent: "claudeCode",
      claude: { model: "claude-opus-4-8", effort: "high" },
      codex: { model: "", effort: "medium" },
      confirmLaunch: false,
    } as T;
  }
  console.debug("[demo] invoke stub:", cmd);
  return undefined as T;
}

/** The current Tauri window → a stub that swallows the theme call `theme.ts`
 *  makes to sync the native titlebar. */
export function getCurrentWindow() {
  return {
    setTheme: async () => {},
  };
}

/** `@tauri-apps/plugin-dialog` → the native folder picker. The demo never opens
 *  a real project (it runs on fixtures), so the picker resolves to nothing. */
export async function open(_opts?: unknown): Promise<string | null> {
  return null;
}

/** `@tauri-apps/api/event` → backend event stream. Nothing streams in the demo
 *  (scenes drive state through the film director), so subscribing is a no-op
 *  that returns an unlisten fn. */
export async function listen<T = unknown>(
  _event: string,
  _handler: (e: { payload: T }) => void,
): Promise<() => void> {
  return () => {};
}
