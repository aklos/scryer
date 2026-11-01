/**
 * The concern-gap marker shared by the diagram card and the tree row, under
 * the concern lens: how many lines in a node's subtree serve the lens's
 * concern while no claim there states it, because only claims tagged with it
 * from elsewhere run them. A hollow, dashed counterpart of the lens's filled
 * count: the concern is here, its claim is missing.
 */

import { CircleDashed } from "lucide-react";

export function ConcernGapMark({
  lines,
  concern,
  className = "",
}: {
  lines: number | undefined;
  concern: string;
  className?: string;
}) {
  if (!lines) return null;
  const title = `${lines} line${lines === 1 ? "" : "s"} here serve${lines === 1 ? "s" : ""} [${concern}] with no [${concern}] claim stated here: only tagged claims from elsewhere run ${lines === 1 ? "it" : "them"}`;
  return (
    <span
      data-concern-gap={lines}
      className={`flex shrink-0 items-center gap-px rounded border border-dashed border-[var(--accent)] px-0.5 font-mono text-2xs tabular-nums text-[var(--accent)] ${className}`}
      title={title}
    >
      <CircleDashed className="h-2.5 w-2.5" />
      {lines}
    </span>
  );
}
