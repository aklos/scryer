// `@tauri-apps/plugin-dialog`, as the film's bundle resolves it: the film
// never opens a picker.
export function open(): Promise<null> {
  return Promise.resolve(null);
}
