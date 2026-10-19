// `@tauri-apps/api/event`, as the shots' bundle resolves it: no backend, so
// no events arrive.
export type UnlistenFn = () => void;

export function listen(_name: string, _handler: (e: { payload: unknown }) => void): Promise<UnlistenFn> {
  return Promise.resolve(() => {});
}

export function emit(_name: string, _payload?: unknown): Promise<void> {
  return Promise.resolve();
}
