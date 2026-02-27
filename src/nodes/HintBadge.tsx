import type { Hint } from "../types";

export function HintBadge({ hints }: { nodeId: string; hints: Hint[] }) {
  if (hints.length === 0) return null;

  const hasWarning = hints.some((h) => h.severity === "warning");

  return (
    <div
      className={`absolute -top-1.5 -right-1.5 z-20 w-4 h-4 rounded-full border-2 border-white dark:border-zinc-800 ${
        hasWarning ? "bg-orange-400" : "bg-teal-400"
      }`}
      title={`${hints.length} hint${hints.length > 1 ? "s" : ""}`}
    />
  );
}
