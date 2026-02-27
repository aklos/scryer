import type { NodeProps } from "@xyflow/react";
import type { C4Node as C4NodeType } from "../types";
import { NodeHandles } from "./NodeHandles";

const KIND_LABELS: Record<string, string> = {
  deployment: "Deployment",
  package: "Package",
};

export function GroupNode({ data, selected }: NodeProps<C4NodeType>) {
  const groupKind = (data as Record<string, unknown>).groupKind as string | undefined;
  const kindLabel = groupKind ? KIND_LABELS[groupKind] : undefined;

  return (
    <div className="w-full h-full">
      <div
        className={`w-full h-full rounded-lg ${
          selected ? "bg-zinc-900/10 dark:bg-zinc-300/10" : "bg-zinc-500/5 dark:bg-zinc-400/5"
        }`}
      >
        <div className="px-3 py-1.5 flex items-center gap-1.5">
          {kindLabel && (
            <span className="text-[9px] font-semibold uppercase tracking-wider text-zinc-500 dark:text-zinc-500">
              {kindLabel}
            </span>
          )}
          <span className="text-xs font-medium text-zinc-600 dark:text-zinc-300">
            {data.name}
          </span>
        </div>
      </div>
      <NodeHandles />
    </div>
  );
}
