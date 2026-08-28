import { useEffect, useState } from "react";
import { Loader2, Moon, Sun } from "lucide-react";
import type { Node } from "../viewmodel";
import { fixturePathFor, type PreviewComponentInfo, type PreviewServerState } from "../hooks/usePreviewServer";
import { useDarkMode } from "../hooks/useDarkMode";
import { BTN_AGENT, BTN_ICON, AgentMark, PageSection } from "../pagekit";

// --- visual preview ---------------------------------------------------------

/** Everything the Preview section needs to render a node's live preview: the
 *  shared dev server, the matched component entry, the component's own theme
 *  (decoupled from scryer's chrome), and the iframe URL. */
function useNodePreview(node: Node, server: PreviewServerState, entry: PreviewComponentInfo) {
  // Scryer's chrome theme drives the checkerboard backdrop (the `canvas` param).
  const isDark = useDarkMode();
  // The previewed component has its OWN theme, decoupled from scryer's chrome:
  // it defaults to the chrome theme when the page opens, then is independent so
  // toggling scryer no longer drags the component with it. Best-effort — see the
  // preview server's `previewHtml` for what this can and can't theme.
  const [componentDark, setComponentDark] = useState(isDark);

  const previewUrl = (file: string, exportName: string, fixture?: string) =>
    `${server.url}/__preview?file=${encodeURIComponent(file)}&export=${encodeURIComponent(exportName)}` +
    (fixture ? `&fixture=${encodeURIComponent(fixture)}` : "") +
    `&theme=${componentDark ? "dark" : "light"}` +
    `&canvas=${isDark ? "dark" : "light"}`;

  const watched = { file: entry.file, exportName: entry.exportName };
  const iframeSrc = server.url
    ? previewUrl(watched.file, watched.exportName, fixturePathFor(node.id, entry))
    : null;

  return { componentDark, setComponentDark, watched, iframeSrc };
}

/** The Preview section is only ever mounted for a node the sidecar can render:
 *  `entry` is the mountable export it reported for the node's anchored source
 *  (the page derives that match — it is never a fact the model asserts). */
export function PreviewSection({
  node,
  server,
  entry,
  onFixture,
}: {
  node: Node;
  server: PreviewServerState;
  entry: PreviewComponentInfo;
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
}) {
  const { componentDark, setComponentDark, watched, iframeSrc } =
    useNodePreview(node, server, entry);

  // The preview entry posts its render verdict (ok/empty/error) to the parent
  // window — this drives the B5 "generate preview data" repair path.
  const [report, setReport] = useState<{ status: string; error: string | null; hasFixture: boolean } | null>(null);
  useEffect(() => setReport(null), [watched.file, watched.exportName]);
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      const d = e.data;
      if (d?.type !== "scryer-render" || d.file !== watched.file || d.exportName !== watched.exportName) return;
      setReport({ status: d.status, error: d.error ?? null, hasFixture: !!d.hasFixture });
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [watched.file, watched.exportName]);

  const needsRepair = report != null && (report.status === "empty" || report.status === "error");
  // No real data behind the preview (no per-node fixture, no type-keyed shared
  // fixture) — offer to generate one even when the render is "ok".
  const noFixture = report != null && report.hasFixture === false;

  return (
    <PageSection
      title="Preview"
      right={
        iframeSrc ? (
          <button
            type="button"
            onClick={() => setComponentDark((d) => !d)}
            title={`Preview the component in ${componentDark ? "light" : "dark"} mode`}
            className={BTN_ICON}
          >
            {componentDark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
          </button>
        ) : undefined
      }
    >
      {iframeSrc ? (
        <div className="flex flex-col gap-2">
          <div className="relative overflow-hidden rounded-md border border-[var(--border)]">
            <iframe
              src={iframeSrc}
              title={`Preview: ${node.name}`}
              className="aspect-[16/10] w-full border-0"
              sandbox="allow-scripts allow-same-origin"
            />
            {/* The dev server compiles the component (and its deps) on the
                first hit — 5–10s cold. Cover the blank iframe until the entry
                posts its first render verdict. */}
            {report == null && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-[var(--surface-canvas)]">
                <Loader2 className="h-5 w-5 animate-spin text-[var(--text-muted)]" />
                <span className="font-mono text-2xs text-[var(--text-muted)]">Loading preview…</span>
              </div>
            )}
          </div>
          {(needsRepair || noFixture) && onFixture && (
            <div className="flex items-center gap-3 self-start">
              <span className="font-mono text-2xs text-[var(--text-muted)]">
                {report!.status === "empty"
                  ? "Rendered empty with placeholder props."
                  : report!.status === "error"
                    ? "Render failed with placeholder props."
                    : "Showing placeholder props — no fixture data yet."}
              </span>
              <button
                type="button"
                onClick={() => onFixture(node.id, report!.status, report!.error)}
                className={BTN_AGENT}
              >
                <AgentMark className="" /> Generate preview data
              </button>
            </div>
          )}
        </div>
      ) : null}
    </PageSection>
  );
}
