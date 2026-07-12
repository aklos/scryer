import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Check,
  Eye,
  GitCompare,
  Loader2,
  Moon,
  Send,
  Sun,
  Undo2,
  X,
} from "lucide-react";
import type { Node } from "../viewmodel";
import { matchPreviewComponent, usePreviewServer } from "../hooks/usePreviewServer";
import { useDarkMode } from "../hooks/useDarkMode";
import { Input } from "../ui";
import {
  BTN,
  BTN_AGENT,
  BTN_GO,
  BTN_ICON,
  EYEBROW,
  AgentMark,
  PageSection,
  SegField,
} from "../pagekit";
import type { VariationState } from "./types";

// --- visual preview ---------------------------------------------------------

export function PreviewSection({
  node,
  projectPath,
  sourceFile,
  onFixture,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
}: {
  node: Node;
  projectPath: string | null;
  /** The node's anchored source file (from the source map), used to pick the
   *  matching component export on the preview server. */
  sourceFile?: string;
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
}) {
  const [modalOpen, setModalOpen] = useState(false);
  const prevVarStatus = useRef<string | null>(null);
  useEffect(() => {
    if (variationState?.status === "ready" && prevVarStatus.current === "generating") {
      setModalOpen(true);
    }
    prevVarStatus.current = variationState?.status ?? null;
  }, [variationState?.status]);

  // Deterministic render: the shared dev server serves any component as a
  // virtual entry with synthesized props — no agent, no per-component build.
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
  const theme = componentDark ? "dark" : "light";
  const canvasTheme = isDark ? "dark" : "light";

  // An accepted variation (design intent, status `changed`) overrides the
  // live component until the real code catches up.
  const accepted = node.appearance?.distPath?.endsWith(".tsx")
    ? node.appearance.distPath
    : null;
  const previewUrl = (file: string, exportName: string, fixture?: string) =>
    `${server.url}/__preview?file=${encodeURIComponent(file)}&export=${encodeURIComponent(exportName)}` +
    (fixture ? `&fixture=${encodeURIComponent(fixture)}` : "") +
    `&theme=${theme}` +
    `&canvas=${canvasTheme}`;

  const watched: { file: string; exportName: string } | null = accepted
    ? { file: accepted, exportName: "default" }
    : entry
      ? { file: entry.file, exportName: entry.exportName }
      : null;
  const iframeSrc =
    server.url && watched
      ? previewUrl(watched.file, watched.exportName, accepted ? undefined : `.scryer/preview/fixtures/${node.id}.tsx`)
      : null;

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

  const placeholder =
    server.status === "error"
      ? `Preview server failed: ${server.error}`
      : server.status === "starting" || !server.components
        ? "Starting preview server…"
        : "Can't preview this yet — only web (React/TSX) components render for now.";

  const canEdit = iframeSrc && onStartVariation;
  const needsRepair = report != null && (report.status === "empty" || report.status === "error");
  // No real data behind the preview (no per-node fixture, no type-keyed shared
  // fixture) — offer to generate one even when the render is "ok". Skipped for
  // accepted variations, which render from their own module without fixtures.
  const noFixture = report != null && report.hasFixture === false && !accepted;

  const variationSrcFor = (idx: number) =>
    previewUrl(`.scryer/preview/variations/${node.id}/${idx}.tsx`, "default");

  return (
    <PageSection
      title="Preview"
      editable={!!canEdit}
      editing={modalOpen}
      onToggleEdit={() => setModalOpen(!modalOpen)}
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
              className="h-[400px] w-full border-0"
              sandbox="allow-scripts allow-same-origin"
            />
            {/* The dev server compiles the component (and its deps) on the
                first hit — 5–10s cold. Cover the blank iframe until the entry
                posts its first render verdict. */}
            {report == null && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-[var(--surface-canvas)]">
                <Loader2 className="h-5 w-5 animate-spin text-violet-500 dark:text-violet-400" />
                <span className="text-2xs text-[var(--text-muted)]">Loading preview…</span>
              </div>
            )}
          </div>
          {accepted && (
            <div className="flex items-center gap-2 self-start text-2xs text-amber-700 dark:text-amber-400">
              <GitCompare className="h-3 w-3 shrink-0" />
              <span>
                Showing the accepted design — the component code hasn't been reconciled to it yet.
              </span>
            </div>
          )}
          {(needsRepair || noFixture) && onFixture && (
            <div className="flex items-center gap-3 self-start">
              <span className="text-2xs text-[var(--text-muted)]">
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
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-[var(--border)] bg-[var(--surface-raised)] px-6 py-10">
          <Eye className="h-6 w-6 text-[var(--text-ghost)]" />
          <p className="text-xs text-[var(--text-muted)]">{placeholder}</p>
        </div>
      )}

      {modalOpen && iframeSrc && (
        <VariationModal
          node={node}
          currentSrc={iframeSrc}
          variationSrc={variationSrcFor}
          variationState={variationState}
          onStartVariation={onStartVariation!}
          onAcceptVariation={onAcceptVariation}
          onDiscardVariations={onDiscardVariations}
          onSelectVariation={onSelectVariation}
          onClose={() => setModalOpen(false)}
        />
      )}
    </PageSection>
  );
}

function VariationModal({
  node,
  currentSrc,
  variationSrc,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
  onClose,
}: {
  node: Node;
  /** Live deterministic preview of the component as it exists now. */
  currentSrc: string;
  /** Preview URL for variation `idx` on the shared dev server. */
  variationSrc: (idx: number) => string;
  variationState: VariationState | null;
  onStartVariation: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  onClose: () => void;
}) {
  const [prompt, setPrompt] = useState("");
  const [count, setCount] = useState<1 | 3>(3);
  const generating = variationState?.status === "generating";
  const ready = variationState?.status === "ready";
  const selectedIdx = variationState?.selectedIdx ?? null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleSubmit = () => {
    const value = prompt.trim();
    if (!value || generating) return;
    onStartVariation(
      node.id,
      value,
      count,
      ready && selectedIdx != null ? selectedIdx : undefined,
    );
    setPrompt("");
  };

  const handleAccept = () => {
    if (selectedIdx == null || !onAcceptVariation) return;
    onAcceptVariation(node.id, selectedIdx);
    onClose();
  };

  const handleDiscard = () => {
    onDiscardVariations?.(node.id);
  };

  const varCount = variationState?.count ?? 0;

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onClose} />
      <div className="relative flex max-h-[90vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="text-sm font-medium text-[var(--text)]">
            Plan visual changes — {node.name}
          </span>
          <button
            type="button"
            onClick={onClose}
            className={BTN_ICON}
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto">
          {/* Original preview — always visible */}
          <div className="border-b border-[var(--border-subtle)] px-5 py-4">
            <p className={`mb-2 ${EYEBROW}`}>Current</p>
            <div className="overflow-hidden rounded-md border border-[var(--border-subtle)]">
              <iframe
                src={currentSrc}
                title={`Current: ${node.name}`}
                className="h-[350px] w-full border-0"
                sandbox="allow-scripts allow-same-origin"
              />
            </div>
          </div>

          {/* Prompt bar */}
          <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-5 py-3">
            <Input
              variant="bordered"
              className="min-w-0 flex-1 disabled:opacity-50"
              type="text"
              placeholder={ready && selectedIdx != null ? "Refine the selected variation…" : "Describe visual changes…"}
              value={prompt}
              disabled={generating}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
            />
            <SegField<1 | 3>
              options={[
                { value: 1, label: "1" },
                { value: 3, label: "3" },
              ]}
              value={count}
              disabled={generating}
              onChange={setCount}
            />
            <button
              type="button"
              disabled={generating}
              onClick={handleSubmit}
              className={`${BTN_AGENT} disabled:opacity-50`}
            >
              <Send className="h-3.5 w-3.5" />
              {ready ? "Iterate" : "Generate"}
            </button>
          </div>

          {/* Variations */}
          {generating && (
            <div className="flex flex-col items-center gap-3 px-5 py-12">
              <Loader2 className="h-6 w-6 animate-spin text-violet-500 dark:text-violet-400" />
              <p className="text-sm text-[var(--text-muted)]">
                Generating {variationState!.count} variation{variationState!.count > 1 ? "s" : ""}…
              </p>
              <p className="text-2xs text-[var(--text-ghost)]">
                "{variationState!.prompt}"
              </p>
            </div>
          )}

          {ready && (
            <div className="px-5 py-4">
              <div className="mb-3 flex items-baseline justify-between">
                <p className="text-xs text-[var(--text-muted)]">
                  "{variationState!.prompt}" — click to select
                </p>
                <div className="flex items-center gap-2">
                  {ready && selectedIdx != null && (
                    <button type="button" onClick={handleAccept} className={BTN_GO}>
                      <Check className="h-3.5 w-3.5" /> Accept
                    </button>
                  )}
                  <button type="button" onClick={handleDiscard} className={BTN}>
                    <Undo2 className="h-3.5 w-3.5" /> Discard
                  </button>
                </div>
              </div>
              <div className={`grid gap-3 ${varCount === 1 ? "grid-cols-1 max-w-[600px]" : "grid-cols-3"}`}>
                {Array.from({ length: varCount }, (_, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={() => onSelectVariation?.(selectedIdx === i ? null : i)}
                    className={`flex flex-col gap-1.5 rounded-lg border-2 p-1 transition-colors ${
                      selectedIdx === i
                        ? "border-violet-500 bg-violet-500/5"
                        : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
                    }`}
                  >
                    <div className="overflow-hidden rounded-md">
                      <iframe
                        src={variationSrc(i)}
                        title={`Variation ${i + 1}`}
                        className="pointer-events-none h-[280px] w-full border-0"
                        sandbox="allow-scripts allow-same-origin"
                      />
                    </div>
                    <span className={`text-2xs font-medium ${
                      selectedIdx === i ? "text-violet-500 dark:text-violet-400" : "text-[var(--text-tertiary)]"
                    }`}>
                      {selectedIdx === i ? "✓ " : ""}Variation {i + 1}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
