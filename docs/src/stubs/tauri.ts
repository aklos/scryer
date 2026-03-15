// Stub for @tauri-apps/api/* — lets us import app source files in the docs
// build without pulling in the Tauri desktop runtime.
export function getCurrentWindow() {
  return { setTheme: () => {} };
}
export function invoke() {
  return Promise.resolve();
}
export function getCurrentWebviewWindow() {
  return { setTheme: () => {} };
}
