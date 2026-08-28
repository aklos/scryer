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
  /** What mounts it — the sidecar tags every entry; absent on older sidecars (React). */
  framework?: "react" | "vue";
}

/** The per-node fixture module for a component: a React fixture is a `.tsx`
 *  module (it may build JSX children); a Vue one is a plain `.ts` props module. */
export function fixturePathFor(nodeId: string, entry: PreviewComponentInfo): string {
  return `.scryer/preview/fixtures/${nodeId}.${entry.framework === "vue" ? "ts" : "tsx"}`;
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

/** The Preview-section gate. Preview-ability is DERIVED, never stored on the
 *  model: a node gets a Preview section iff it is a symbol and the sidecar
 *  reports a mountable export matching its name / anchored source file. Null
 *  while the component list is still loading (or the server failed) — the
 *  section simply doesn't exist until the sidecar says it can render. */
export function previewEntryFor(
  node: { kind: string; name: string },
  components: PreviewComponentInfo[] | null,
  sourceFile: string | undefined,
): PreviewComponentInfo | null {
  if (node.kind !== "symbol" || !components) return null;
  return matchPreviewComponent(components, node.name, sourceFile);
}

/** Pick the preview entry for a model node. A symbol is named by its exact
 *  code identifier, so only an export of that name counts — a same-file export
 *  under another name (the helper next to the component) never does. The
 *  anchored file breaks a tie between same-named exports in different files. */
export function matchPreviewComponent(
  components: PreviewComponentInfo[],
  nodeName: string,
  sourceFile: string | undefined,
): PreviewComponentInfo | null {
  const named = components.filter((c) => c.displayName === nodeName || c.exportName === nodeName);
  return named.find((c) => c.file === sourceFile) ?? named[0] ?? null;
}

/** Every node id the sidecar can currently render — the derived set the tree
 *  and diagram mark with the preview glyph. Empty until the component list
 *  loads. `anchorOf` gives a node's anchored source file (from the source map). */
export function previewableNodeIds(
  nodes: readonly { id: string; kind: string; name: string }[],
  components: PreviewComponentInfo[] | null,
  anchorOf: (nodeId: string) => string | undefined,
): Set<string> {
  const out = new Set<string>();
  if (!components?.length) return out;
  for (const n of nodes) if (previewEntryFor(n, components, anchorOf(n.id))) out.add(n.id);
  return out;
}
