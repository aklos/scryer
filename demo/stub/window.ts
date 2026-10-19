// `@tauri-apps/api/window`, as the film's bundle resolves it: a window that
// is never maximised and ignores theme and size requests.
const win = {
  isMaximized: () => Promise.resolve(false),
  onResized: () => Promise.resolve(() => {}),
  minimize: () => Promise.resolve(),
  toggleMaximize: () => Promise.resolve(),
  close: () => Promise.resolve(),
  setTheme: () => Promise.resolve(),
  startDragging: () => Promise.resolve(),
};

export function getCurrentWindow() {
  return win;
}
