// `@tauri-apps/api/core`, as the shots' bundle resolves it. A shot mounts one
// component with its state as props, so nothing it renders should call the
// backend; a call that slips through answers null rather than hang the frame.
export function invoke<T>(_cmd: string, _args?: Record<string, unknown>): Promise<T> {
  return Promise.resolve(null as T);
}

export function convertFileSrc(path: string): string {
  return path;
}
