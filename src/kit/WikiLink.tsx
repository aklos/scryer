import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { ArrowDownLeft, ArrowUpRight } from "lucide-react";

// --- wiki links ---------------------------------------------------------------

/**
 * An inline cross-reference to another node/group's page. A plain blue wikilink:
 * every target is a real, navigable page, so link colour no longer tries to
 * encode plan-vs-built (that reads on the target's own page). `muted` dims a
 * link that isn't declared (a code-suggested candidate).
 */
export function WikiLink({
  name,
  Icon,
  onClick,
  dir,
  muted = false,
}: {
  name: string;
  Icon?: ComponentType<LucideProps>;
  onClick: () => void;
  dir?: "in" | "out";
  muted?: boolean;
}) {
  const Arrow = dir === "out" ? ArrowUpRight : dir === "in" ? ArrowDownLeft : null;
  const color = muted ? "text-[var(--text-muted)]" : "text-blue-700 dark:text-blue-400";
  return (
    <button
      type="button"
      onClick={onClick}
      className={`group/wl inline-flex max-w-full items-center gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-[var(--surface-hover)] ${color}`}
    >
      {Arrow && <Arrow className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />}
      {Icon && <Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />}
      <span className="truncate text-sm group-hover/wl:underline">
        {name || "Untitled"}
      </span>
    </button>
  );
}

/** Scroll an element into view and flash it briefly — the citation-jump
 *  affordance between claims and their source hunks. */
export function jumpTo(elementId: string) {
  const el = document.getElementById(elementId);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.remove("flash-target");
  // Force a reflow so re-adding the class restarts the animation.
  void el.offsetWidth;
  el.classList.add("flash-target");
  window.setTimeout(() => el.classList.remove("flash-target"), 1400);
}
