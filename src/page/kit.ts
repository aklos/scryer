import { useState } from "react";

// Row grid for the mono lanes: marker | index | content. The edit controls
// (CTL) float over the right edge as an absolute overlay (so `relative`), which
// takes no layout space — read↔edit stays the same width and never reflows.
export const RESP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";
export const PROP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";

// The mockup's `.ctl` overlay, scoped to a statement (`/srow`) or directive
// (`/drow`) line so each line reveals only its own controls on hover. Same
// gradient float as the shared CTL; `not-italic` keeps buttons upright on the
// italic directive rows.
const CTL_BASE =
  "pointer-events-none invisible absolute inset-y-0 -right-1 z-10 flex items-center gap-1.5 not-italic pl-9 pr-1 [background-image:linear-gradient(90deg,transparent,var(--surface-tint)_28px)]";
export const CTL_SROW = `${CTL_BASE} group-hover/srow:visible`;
export const CTL_DROW = `${CTL_BASE} group-hover/drow:visible`;
// Header gauge chip — completeness / test-backing readouts on the type line.
// Bordered mono chips: instruments, not prose.
export const GAUGE_CHIP =
  "flex shrink-0 items-center gap-1 rounded border border-[var(--border)] px-1.5 py-px font-mono text-2xs tabular-nums text-[var(--text-tertiary)]";

// Full-cell field highlight: dim on line hover; the focused field drops onto
// the recessed field surface (Editable adds the accent ring + full-text lift).
// The hover half stands alone for mirror-mode statement fields, where the
// focus surface lives on Editable's own wrapper (focus-within) instead.
export const STMT_HL_HOVER =
  "group-hover/srow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)]";
export const STMT_HL = `${STMT_HL_HOVER} focus:bg-[var(--surface-field)]`;
export const DIR_HL =
  "group-hover/drow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)] focus:bg-[var(--surface-field)]";

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
