/**
 * Deterministic preview server access (Track B). Asks the backend to start
 * (or reuse) the project's shared Vite dev server and exposes its URL plus
 * the list of renderable components it discovered. No agent involved —
 * components render by pointing an iframe at
 * `{url}/__preview?file=...&export=...`.
 */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface PreviewComponentInfo {
  file: string;
  exportName: string;
  displayName: string;
  warnings: string[];
}

export interface PreviewServerState {
  status: "starting" | "ready" | "error";
  url: string | null;
  /** Discovered components, null while loading. */
  components: PreviewComponentInfo[] | null;
  error: string | null;
}

export function usePreviewServer(projectPath: string | null): PreviewServerState {
  const [state, setState] = useState<PreviewServerState>({
    status: "starting",
    url: null,
    components: null,
    error: null,
  });

  useEffect(() => {
    if (!projectPath) return;
    let cancelled = false;
    setState({ status: "starting", url: null, components: null, error: null });
    (async () => {
      try {
        const url = await invoke<string>("ensure_preview_server", { cwd: projectPath });
        if (cancelled) return;
        setState({ status: "ready", url, components: null, error: null });
        const res = await fetch(`${url}/__components.json`);
        const data = await res.json();
        if (cancelled) return;
        setState({ status: "ready", url, components: data.components ?? [], error: null });
      } catch (e) {
        if (!cancelled) setState({ status: "error", url: null, components: null, error: String(e) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  return state;
}

/** Pick the preview entry for a model node: match the node's source file and
 *  name when possible, fall back to either alone. */
export function matchPreviewComponent(
  components: PreviewComponentInfo[],
  nodeName: string,
  sourceFile: string | undefined,
): PreviewComponentInfo | null {
  return (
    components.find((c) => c.file === sourceFile && c.displayName === nodeName) ??
    components.find((c) => c.displayName === nodeName) ??
    (sourceFile ? components.find((c) => c.file === sourceFile) : undefined) ??
    null
  );
}
