/**
 * Export-viewer Tauri shim.
 *
 * The standalone HTML export lifts the real `DiagramView` (and, transitively,
 * `theme.ts`) into a plain browser with no Rust backend behind it. A couple of
 * those modules import `@tauri-apps/api/*`; the export Vite config aliases those
 * imports to THIS file so they resolve to harmless no-ops. The app's own build
 * never sees this alias — the export stays non-invasive, mirroring the demo's
 * `demo/tauri-stub.ts` (but without its fixtures dependency).
 */

/** Backend command bridge → no-op. The exported viewer is read-only and serves
 *  everything it needs from the baked-in model, so no command ever runs. */
export async function invoke<T = unknown>(): Promise<T> {
  return undefined as T;
}

/** Current Tauri window → swallows the `setTheme` call `theme.ts` makes to sync
 *  the native titlebar. */
export function getCurrentWindow() {
  return {
    setTheme: async () => {},
  };
}

/** Native folder picker → never opens; the viewer runs entirely on its baked
 *  model. */
export async function open(): Promise<string | null> {
  return null;
}

/** Backend event stream → nothing streams in a static export, so subscribing is
 *  a no-op returning an unlisten fn. */
export async function listen(): Promise<() => void> {
  return () => {};
}
