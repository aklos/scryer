import { useState } from "react";

// Row grid for the mono lanes: marker | index | content. The edit controls
// (CTL) float over the right edge as an absolute overlay (so `relative`), which
// takes no layout space — read↔edit stays the same width and never reflows.
export const RESP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";
export const PROP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";

// A chip on the page's type line — style, layer, claims. One size and one
// shape for all of them; each leads with a muted word saying what it is.
export const TYPE_CHIP =
  "flex h-5 shrink-0 items-center gap-1.5 rounded border border-[var(--border)] px-1.5 font-mono text-2xs leading-none tabular-nums text-[var(--text-secondary)]";

// Full-cell field highlight: dim on line hover; the focused field drops onto
// the recessed field surface (Editable adds the accent ring + full-text lift).
// The hover half stands alone for mirror-mode statement fields, where the
// focus surface lives on Editable's own wrapper (focus-within) instead.
export const STMT_HL_HOVER =
  "group-hover/srow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)]";
export const STMT_HL = `${STMT_HL_HOVER} focus:bg-[var(--surface-field)]`;

/** Per-section edit toggles for one page. Edits inside a section accumulate
 *  in a local draft and persist only on Done; Cancel discards the draft —
 *  nothing reaches the model (or disk) until an explicit commit. */
export function useEditSections() {
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  return {
    isEditing: (key: string) => open.has(key),
    toggle: (key: string) =>
      setOpen((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      }),
  };
}
