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

/** Backend command bridge → no-op. Returns undefined for every command; scenes
 *  feed real state through props/fixtures instead. */
export async function invoke<T = unknown>(cmd: string, _args?: unknown): Promise<T> {
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
