import { useEffect, useState } from "react";
import { GitCompare, Loader2, Moon, Sun } from "lucide-react";
import type { Node } from "../viewmodel";
import { matchPreviewComponent, usePreviewServer } from "../hooks/usePreviewServer";
import { useDarkMode } from "../hooks/useDarkMode";
import {
  BTN_AGENT,
  BTN_ICON,
  LINK,
  AgentMark,
  Empty,
  PageSection,
} from "../pagekit";

// --- visual preview ---------------------------------------------------------

/** Everything a surface needs to render a node's live preview: the shared dev
 *  server, the matched component entry, the accepted-variation override, the
 *  component's own theme (decoupled from scryer's chrome), and URL builders
 *  for the main view and for variation modules. Shared by the read-mode
 *  Preview section and the appearance workspace. */
export function useNodePreview(node: Node, projectPath: string | null, sourceFile?: string) {
  const server = usePreviewServer(projectPath);
  const entry = server.components
    ? matchPreviewComponent(server.components, node.name, sourceFile)
    : null;
  // Scryer's chrome theme drives the checkerboard backdrop (the `canvas` param).
  const isDark = useDarkMode();
  // The previewed component has its OWN theme, decoupled from scryer's chrome:
  // it defaults to the chrome theme when the page opens, then is independent so
  // toggling scryer no longer drags the component with it. Best-effort — see the
  // preview server's `previewHtml` for what this can and can't theme.
  const [componentDark, setComponentDark] = useState(isDark);

  // An accepted variation (design intent, status `changed`) overrides the
  // live component until the real code catches up.
  const accepted = node.appearance?.distPath?.endsWith(".tsx")
    ? node.appearance.distPath
    : null;
  const previewUrl = (file: string, exportName: string, fixture?: string) =>
    `${server.url}/__preview?file=${encodeURIComponent(file)}&export=${encodeURIComponent(exportName)}` +
    (fixture ? `&fixture=${encodeURIComponent(fixture)}` : "") +
    `&theme=${componentDark ? "dark" : "light"}` +
    `&canvas=${isDark ? "dark" : "light"}`;

  const watched: { file: string; exportName: string } | null = accepted
    ? { file: accepted, exportName: "default" }
    : entry
      ? { file: entry.file, exportName: entry.exportName }
      : null;
  const iframeSrc =
    server.url && watched
      ? previewUrl(watched.file, watched.exportName, accepted ? undefined : `.scryer/preview/fixtures/${node.id}.tsx`)
      : null;

  const variationSrcFor = (idx: number) =>
    previewUrl(`.scryer/preview/variations/${node.id}/${idx}.tsx`, "default");

  return { server, accepted, componentDark, setComponentDark, watched, iframeSrc, variationSrcFor };
}

export function PreviewSection({
  node,
  projectPath,
  sourceFile,
  onFixture,
  variationsReady,
  onEditAppearance,
}: {
  node: Node;
  projectPath: string | null;
  /** The node's anchored source file (from the source map), used to pick the
   *  matching component export on the preview server. */
  sourceFile?: string;
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  /** Variations finished generating while the workspace was closed. */
  variationsReady?: boolean;
  onEditAppearance?: () => void;
}) {
  const { accepted, componentDark, setComponentDark, watched, iframeSrc, server } =
    useNodePreview(node, projectPath, sourceFile);

  // The preview entry posts its render verdict (ok/empty/error) to the parent
  // window — this drives the B5 "generate preview data" repair path.
  const [report, setReport] = useState<{ status: string; error: string | null; hasFixture: boolean } | null>(null);
  useEffect(() => setReport(null), [watched?.file, watched?.exportName]);
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      const d = e.data;
      if (d?.type !== "scryer-render" || !watched || d.file !== watched.file || d.exportName !== watched.exportName) return;
      setReport({ status: d.status, error: d.error ?? null, hasFixture: !!d.hasFixture });
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [watched?.file, watched?.exportName]);

  const canEdit = !!iframeSrc && !!onEditAppearance;
  const needsRepair = report != null && (report.status === "empty" || report.status === "error");
  // No real data behind the preview (no per-node fixture, no type-keyed shared
  // fixture) — offer to generate one even when the render is "ok". Skipped for
  // accepted variations, which render from their own module without fixtures.
  const noFixture = report != null && report.hasFixture === false && !accepted;

  return (
    <PageSection
      title="Preview"
      editable={canEdit}
      editing={false}
      onToggleEdit={onEditAppearance}
      right={
        iframeSrc ? (
          <span className="flex items-center gap-2">
            {variationsReady && (
              <button type="button" onClick={onEditAppearance} className={BTN_AGENT}>
                <AgentMark /> Variations ready
              </button>
            )}
            <button
              type="button"
              onClick={() => setComponentDark((d) => !d)}
              title={`Preview the component in ${componentDark ? "light" : "dark"} mode`}
              className={BTN_ICON}
            >
              {componentDark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
            </button>
          </span>
        ) : undefined
      }
    >
      {iframeSrc ? (
        <div className="flex flex-col gap-2">
          <div className="relative overflow-hidden rounded-md border border-[var(--border)]">
            <iframe
              src={iframeSrc}
              title={`Preview: ${node.name}`}
              className="h-[400px] w-full border-0"
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
          {accepted && (
            <div className="flex items-center gap-2 self-start font-mono text-2xs text-amber-700 dark:text-amber-400">
              <GitCompare className="h-3 w-3 shrink-0" />
              <span>
                Showing the accepted design — the component code hasn't been reconciled to it yet.
              </span>
            </div>
          )}
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
      ) : server.status === "error" ? (
        <PreviewError error={server.error ?? "unknown error"} />
      ) : server.status === "starting" || !server.components ? (
        <Empty>
          <span className="inline-flex items-center gap-2 leading-none">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Starting preview server…
          </span>
        </Empty>
      ) : (
        <Empty>No preview — only web (React/TSX) components render for now.</Empty>
      )}
    </PageSection>
  );
}

/** The server failure state in the section's quiet register: one mono line
 *  carrying the failure's gist, with the raw output (stack trace, stderr tail)
 *  behind a disclosure instead of dumped into the page. */
function PreviewError({ error }: { error: string }) {
  const [open, setOpen] = useState(false);
  const gist = errorGist(error);
  return (
    <Empty>
      <p>
        Preview server failed{gist ? <> — {gist}</> : null}.{" "}
        <button type="button" onClick={() => setOpen(!open)} className={LINK}>
          {open ? "hide details" : "details"}
        </button>
      </p>
      {open && (
        <pre className="mt-2 max-h-60 select-text overflow-auto overscroll-contain whitespace-pre-wrap rounded-md border border-[var(--border)] bg-[var(--surface-inset)] p-3 text-2xs leading-relaxed">
          {error}
        </pre>
      )}
    </Empty>
  );
}

/** The one line worth reading in a server failure: the thrown error ("Error:
 *  Cannot find module 'vite'") when the text contains one, else the first
 *  non-empty line. A bare "Error:" prefix is dropped as redundant after
 *  "Preview server failed —"; named types (TypeError…) are kept. */
function errorGist(error: string): string {
  const lines = error
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  const thrown = lines.find((l) => /^[A-Za-z]*Error:/.test(l));
  return (thrown ?? lines[0] ?? "").replace(/^Error:\s*/, "");
}
