/**
 * The structural-violation marker shared by the diagram card and the tree row:
 * how many violations the structural check charges inside a node's subtree, so
 * the way down to them is visible from any altitude. Orange when every one is
 * a container with no declared style (nothing chosen yet), red otherwise —
 * the same split as the node page's notice.
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
  const unstyledOnly = violations!.every((v) => v.kind === "unstyled");
  const tone = unstyledOnly
    ? "text-orange-600 dark:text-orange-400"
    : "text-red-600 dark:text-red-400";
  const title = unstyledOnly
    ? `${n} container${n === 1 ? "" : "s"} in this subtree declare${n === 1 ? "s" : ""} no architectural style`
    : `${n} structural violation${n === 1 ? "" : "s"} in this subtree`;
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
