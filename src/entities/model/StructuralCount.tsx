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
}: {
  violations: readonly StyleViolation[] | undefined;
  className?: string;
}) {
  const n = violations?.length ?? 0;
  if (n === 0) return null;
  const tone = "text-red-600 dark:text-red-400";
  const title = `${n} structural violation${n === 1 ? "" : "s"} in this subtree`;
  return (
    <span
      data-structural-count={n}
      className={`flex shrink-0 items-center gap-px font-mono text-2xs tabular-nums ${tone} ${className}`}
      title={title}
    >
      <Flag className="h-3 w-3" />
      {n}
    </span>
  );
}
