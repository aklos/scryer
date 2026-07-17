/**
 * Appearance workspace — the node page's takeover mode for visual iteration.
 * Replaces the article + detail rail with a design surface: one large live
 * stage (the component, fully interactive), a filmstrip rail of Current +
 * generated variations, and a bottom prompt bar that asks the agent for the
 * next round. No modal: entering/leaving is a page mode, the header and tabs
 * stay put.
 *
 * Staging model: `selectedIdx` (owned by the app, shared with the agent
 * session) is the variation on stage — `null` stages Current. The staged
 * variation is also what Accept persists and what a new prompt iterates from.
 */

import { useEffect, useState } from "react";
import { Check, GitCompare, Loader2, Moon, Send, Sun, Undo2, X } from "lucide-react";
import type { Node } from "../viewmodel";
import { AgentMark, BTN, BTN_GO, BTN_ICON, EYEBROW, PAGE_COL } from "../pagekit";
import type { VariationState } from "./types";
import { useNodePreview } from "./PreviewSection";

export function AppearanceWorkspace({
  node,
  projectPath,
  sourceFile,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
  onClose,
}: {
  node: Node;
  projectPath: string | null;
  sourceFile?: string;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  onClose: () => void;
}) {
  const preview = useNodePreview(node, projectPath, sourceFile);
  const [prompt, setPrompt] = useState("");
  const [count, setCount] = useState<1 | 3>(3);
  const generating = variationState?.status === "generating";
  const ready = variationState?.status === "ready";
  const varCount = ready ? variationState!.count : 0;
  const stagedIdx = ready ? (variationState!.selectedIdx ?? null) : null;
  // Hold-to-flip: while held (button or the C key), the stage shows Current
  // under the staged variation — an instant swap, both iframes stay mounted.
  const [comparing, setComparing] = useState(false);

  // Esc leaves the mode; ↑/↓ walk the filmstrip (Current, then variations)
  // and held C compares — unless focus is in the prompt input.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if ((e.key === "ArrowUp" || e.key === "ArrowDown") && varCount > 0) {
        e.preventDefault();
        const order: (number | null)[] = [null, ...Array.from({ length: varCount }, (_, i) => i)];
        const at = order.indexOf(stagedIdx);
        const next = order[Math.min(order.length - 1, Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)))];
        onSelectVariation?.(next);
      }
      if ((e.key === "c" || e.key === "C") && stagedIdx != null) setComparing(true);
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "c" || e.key === "C") setComparing(false);
    };
    // Focus moving into the stage iframe blurs the window and would swallow
    // the C keyup — never leave the flip stuck.
    const onBlur = () => setComparing(false);
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [onClose, onSelectVariation, stagedIdx, varCount]);

  const handleSubmit = () => {
    const value = prompt.trim();
    if (!value || generating || !onStartVariation) return;
    onStartVariation(node.id, value, count, stagedIdx ?? undefined);
    setPrompt("");
  };

  const handleAccept = () => {
    if (stagedIdx == null || !onAcceptVariation) return;
    onAcceptVariation(node.id, stagedIdx);
    onClose();
  };

  if (!preview.iframeSrc) {
    // The server died or the component un-matched while editing — nothing to
    // stage, so fall back to the read view rather than a dead surface.
    return (
      <div className="flex h-full items-center justify-center text-xs text-[var(--text-muted)]">
        <button type="button" onClick={onClose} className={BTN}>
          Preview unavailable — back to the page
        </button>
      </div>
    );
  }

  // With a variation staged, Current stays mounted underneath so the compare
  // flip is instant — no iframe reload, the diff pops.
  const showCurrent = stagedIdx == null || comparing;

  return (
    <div className={`${PAGE_COL} flex h-full min-h-0 flex-col gap-3 pb-6 pt-[18px]`}>
      {/* Workspace chrome: what mode this is, on the left; stage controls right. */}
      <div className="flex items-center justify-between gap-2">
        <span className={EYEBROW}>
          Edit appearance
          <span className="ml-1.5 font-mono font-normal normal-case tracking-normal text-[var(--text-ghost)]">
            {node.name}
          </span>
        </span>
        <span className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => preview.setComponentDark((d) => !d)}
            title={`Preview the component in ${preview.componentDark ? "light" : "dark"} mode`}
            className={BTN_ICON}
          >
            {preview.componentDark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
          </button>
          <button type="button" onClick={onClose} className={BTN}>
            Done
          </button>
        </span>
      </div>

      <div className="flex min-h-0 flex-1 gap-4">
        {/* Stage: whatever is staged, live and fully interactive. */}
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          <div className="relative min-h-0 flex-1 overflow-hidden rounded-md border border-[var(--border)]">
            <iframe
              src={preview.iframeSrc}
              title={`Current: ${node.name}`}
              className={`absolute inset-0 h-full w-full border-0 ${
                showCurrent ? "" : "pointer-events-none opacity-0"
              }`}
              sandbox="allow-scripts allow-same-origin"
            />
            {stagedIdx != null && (
              <iframe
                key={stagedIdx}
                src={preview.variationSrcFor(stagedIdx)}
                title={`Variation ${stagedIdx + 1}: ${node.name}`}
                className={`absolute inset-0 h-full w-full border-0 ${
                  comparing ? "pointer-events-none opacity-0" : ""
                }`}
                sandbox="allow-scripts allow-same-origin"
              />
            )}
            {stagedIdx != null && (
              <button
                type="button"
                title="Hold to see the current design (or hold C)"
                onPointerDown={() => setComparing(true)}
                onPointerUp={() => setComparing(false)}
                onPointerLeave={() => setComparing(false)}
                onPointerCancel={() => setComparing(false)}
                className={`absolute right-2 top-2 inline-flex select-none items-center gap-1.5 rounded border px-2.5 py-1 text-2xs shadow-sm transition-colors ${
                  comparing
                    ? "border-amber-500/60 bg-amber-500/15 text-amber-700 dark:text-amber-400"
                    : "border-[var(--border-strong)] bg-[var(--surface-raised)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
                }`}
              >
                <GitCompare className="h-3 w-3" />
                {comparing ? "Current design" : "Hold to compare"}
                <kbd className="rounded border border-current/30 px-1 font-mono text-[10px] leading-4 opacity-70">
                  C
                </kbd>
              </button>
            )}
          </div>

          {/* Composer: one surface — iteration-base chip, prompt, controls.
              A staged variation is the explicit base; the chip says so and ×
              reverts to iterating from Current. */}
          <div
            className={`flex shrink-0 flex-col rounded-lg border bg-[var(--surface-field)] transition-colors ${
              generating
                ? "border-[var(--border)] opacity-80"
                : "border-[var(--border-strong)] focus-within:border-[var(--accent)] focus-within:ring-1 focus-within:ring-[var(--accent)]"
            }`}
          >
            {stagedIdx != null && (
              <div className="flex px-2.5 pt-2">
                <span className="inline-flex items-center gap-1 rounded-md border border-[color-mix(in_srgb,var(--accent)_40%,transparent)] bg-[var(--accent-soft)] py-0.5 pl-2 pr-1 text-2xs text-[var(--accent)]">
                  from Variation {stagedIdx + 1}
                  <button
                    type="button"
                    title="Iterate from the current design instead"
                    onClick={() => onSelectVariation?.(null)}
                    className="rounded p-0.5 hover:bg-[color-mix(in_srgb,var(--accent)_18%,transparent)]"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </span>
              </div>
            )}
            <input
              type="text"
              className="w-full bg-transparent px-3 pb-1 pt-2.5 text-sm text-[var(--text)] outline-none placeholder:text-[var(--text-ghost)] disabled:cursor-default"
              placeholder={
                generating
                  ? `Generating — "${variationState!.prompt}"`
                  : stagedIdx != null
                    ? "Describe changes to this variation…"
                    : "Describe visual changes…"
              }
              value={prompt}
              disabled={generating}
              autoFocus
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSubmit();
              }}
            />
            <div className="flex items-center justify-between px-2.5 pb-2 pt-1">
              <div className="flex items-center gap-0.5" title="How many variations to generate">
                {([1, 3] as const).map((n) => (
                  <button
                    key={n}
                    type="button"
                    disabled={generating}
                    onClick={() => setCount(n)}
                    className={`rounded px-1.5 py-0.5 font-mono text-2xs transition-colors ${
                      count === n
                        ? "bg-[var(--accent-soft)] font-medium text-[var(--accent)]"
                        : "text-[var(--text-ghost)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
                    }`}
                  >
                    {n}
                  </button>
                ))}
                <span className="ml-1 text-2xs text-[var(--text-ghost)]">
                  variation{count > 1 ? "s" : ""}
                </span>
              </div>
              <button
                type="button"
                disabled={generating || !prompt.trim()}
                onClick={handleSubmit}
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1 text-2xs font-medium text-white transition-colors ${
                  generating || !prompt.trim()
                    ? "cursor-default bg-violet-600/40 dark:bg-violet-500/30"
                    : "bg-violet-600 hover:bg-violet-500 dark:bg-violet-500 dark:hover:bg-violet-400"
                }`}
              >
                {generating ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Send className="h-3.5 w-3.5" />
                )}
                {generating ? "Generating…" : stagedIdx != null ? "Iterate" : "Generate"}
              </button>
            </div>
          </div>
        </div>

        {/* Filmstrip: Current plus this round's variations; click to stage. */}
        <aside className="flex w-[300px] shrink-0 flex-col gap-2">
          <div className="flex min-h-[22px] items-center justify-between gap-2">
            <span className={EYEBROW}>Versions</span>
            {ready && (
              <span className="flex items-center gap-1.5">
                {stagedIdx != null && onAcceptVariation && (
                  <button type="button" onClick={handleAccept} className={BTN_GO}>
                    <Check className="h-3.5 w-3.5" /> Accept
                  </button>
                )}
                {onDiscardVariations && (
                  <button
                    type="button"
                    title="Discard all variations"
                    onClick={() => onDiscardVariations(node.id)}
                    className={BTN}
                  >
                    <Undo2 className="h-3.5 w-3.5" /> Discard
                  </button>
                )}
              </span>
            )}
          </div>

          <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pr-0.5">
            <FilmCard
              label="Current"
              src={preview.iframeSrc}
              staged={stagedIdx == null}
              onStage={() => onSelectVariation?.(null)}
            />

            {ready && (
              <p className="font-mono text-2xs text-[var(--text-ghost)]">
                "{variationState!.prompt}"
              </p>
            )}
            {ready &&
              Array.from({ length: varCount }, (_, i) => (
                <FilmCard
                  key={i}
                  label={`Variation ${i + 1}`}
                  src={preview.variationSrcFor(i)}
                  staged={stagedIdx === i}
                  onStage={() => onSelectVariation?.(i)}
                />
              ))}

            {generating &&
              Array.from({ length: variationState!.count }, (_, i) => (
                <div
                  key={i}
                  className="flex h-[120px] shrink-0 flex-col items-center justify-center gap-2 rounded-md border border-dashed border-[var(--border-strong)] text-2xs text-[var(--text-muted)]"
                >
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-violet-500 dark:text-violet-400" />
                  Variation {i + 1}
                </div>
              ))}
            {generating && (
              <p className="flex items-center gap-1.5 font-mono text-2xs leading-none text-[var(--text-ghost)]">
                <AgentMark /> "{variationState!.prompt}"
              </p>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

/** One filmstrip entry: a live miniature (inert — the stage is where you
 *  interact) that stages itself on click. */
function FilmCard({
  label,
  src,
  staged,
  onStage,
}: {
  label: string;
  src: string;
  staged: boolean;
  onStage: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onStage}
      className={`shrink-0 rounded-md border-2 p-1 text-left transition-colors ${
        staged
          ? "border-[var(--accent)] bg-[var(--accent-soft)]"
          : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
      }`}
    >
      <div className="pointer-events-none h-[150px] overflow-hidden rounded">
        <iframe
          src={src}
          title={label}
          className="h-full w-full border-0"
          sandbox="allow-scripts allow-same-origin"
          tabIndex={-1}
        />
      </div>
      <p
        className={`px-1 pt-1 font-mono text-2xs ${
          staged ? "font-medium text-[var(--accent)]" : "text-[var(--text-tertiary)]"
        }`}
      >
        {label}
      </p>
    </button>
  );
}
