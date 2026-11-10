/**
 * The structural-violation marker shared by the diagram card and the tree row:
 * how many violations the structural check charges inside a node's subtree, so
 * the way down to them is visible from any altitude. A container with no
 * style chosen is not among them: nothing was declared, so nothing breaks.
 */

import { Flag } from "lucide-react";
import type { StyleViolation } from "./health";

export function StructuralCount({
  violations,
  className = "",
  mark = "flag",
}: {
  violations: readonly StyleViolation[] | undefined;
  className?: string;
  /** `flag`: the flag and the subtree's count (the map's cards). In the
   *  tree's column, `here`: a chip, the violations charged to the node
   *  itself; `below`: the bare number, the ones somewhere inside it. */
  mark?: "flag" | "here" | "below";
}) {
  const n = violations?.length ?? 0;
  if (n === 0) return null;
  const s = n === 1 ? "" : "s";
  const title =
    mark === "here"
      ? `${n} structural violation${s} in this component's own code: it breaks its container's style. Its page lists each one and the file to fix.`
      : mark === "below"
        ? `${n} structural violation${s} inside, in the components below. Expand to find them.`
        : `${n} structural violation${s} in this subtree`;
  if (mark === "here")
    return (
      <span
        data-structural-count={n}
        data-structural-mark="here"
        className={`flex h-[15px] min-w-[14px] shrink-0 items-center justify-center rounded-[4px] bg-red-600 px-[2px] font-mono text-2xs font-semibold leading-none tabular-nums text-[var(--surface)] dark:bg-red-400 ${className}`}
        title={title}
      >
        {n}
      </span>
    );
  return (
    <span
      data-structural-count={n}
      data-structural-mark={mark}
      className={`flex shrink-0 items-center gap-px font-mono text-2xs tabular-nums text-red-600 dark:text-red-400 ${className}`}
      title={title}
    >
      {mark === "flag" && <Flag className="h-3 w-3" />}
      {n}
    </span>
  );
}
