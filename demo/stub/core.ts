// `@tauri-apps/api/core`, as the film's bundle resolves it. A beat mounts
// one component with its state as props; the few that fetch (the landing
// page) are answered from what the beat registered here, and anything else
// answers null rather than hang the frame.

type Handler = (args: Record<string, unknown>) => unknown;
const handlers = new Map<string, Handler>();

/** Answer `cmd` for the components the film mounts. */
export function answer(cmd: string, handler: Handler) {
  handlers.set(cmd, handler);
}

export function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return Promise.resolve((handlers.get(cmd)?.(args) ?? null) as T);
}

export function convertFileSrc(path: string): string {
  return path;
}
