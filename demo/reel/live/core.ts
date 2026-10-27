// `@tauri-apps/api/core`, as the reel's bundle resolves it.
export { invoke } from "./backend";

export function convertFileSrc(path: string): string {
  return path;
}
