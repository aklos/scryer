/**
 * Status model — one combined enum across both layers.
 *
 *  planned     — in the model, not in the code yet
 *  placeholder — a hollow stub exists in the code
 *  implemented — real code, does the thing, not yet checked
 *  verified    — real code, confirmed conformant
 *  removing    — slated for removal/relocation, still in the code
 *  vagrant     — in the code, NOT in the model (observation-layer only)
 */

export type Status =
  | "planned"
  | "placeholder"
  | "implemented"
  | "verified"
  | "removing"
  | "vagrant";

export const STATUS_COLORS: Record<
  Status,
  { icon: string; dot: string; badge: string; label: string }
> = {
  planned: {
    icon: "text-blue-500 dark:text-blue-400",
    dot: "bg-blue-500 dark:bg-blue-400",
    badge: "text-blue-950 dark:text-blue-950",
    label: "Planned",
  },
  placeholder: {
    icon: "text-violet-500 dark:text-violet-400",
    dot: "bg-violet-500 dark:bg-violet-400",
    badge: "text-violet-950 dark:text-violet-950",
    label: "Placeholder",
  },
  implemented: {
    icon: "text-amber-500 dark:text-amber-400",
    dot: "bg-amber-500 dark:bg-amber-400",
    badge: "text-amber-950 dark:text-amber-950",
    label: "Implemented",
  },
  verified: {
    icon: "text-emerald-500 dark:text-emerald-400",
    dot: "bg-emerald-500 dark:bg-emerald-400",
    badge: "text-emerald-950 dark:text-emerald-950",
    label: "Verified",
  },
  removing: {
    icon: "text-zinc-400 dark:text-zinc-500",
    dot: "bg-zinc-400 dark:bg-zinc-500",
    badge: "text-zinc-950 dark:text-zinc-950",
    label: "Removing",
  },
  vagrant: {
    icon: "text-red-500 dark:text-red-400",
    dot: "bg-red-500 dark:bg-red-400",
    badge: "text-red-950 dark:text-red-950",
    label: "Vagrant",
  },
};

const LIFE_RANK: Record<string, number> = {
  planned: 0,
  placeholder: 1,
  implemented: 2,
  verified: 3,
};

/**
 * Roll a set of responsibility statuses up to one card status. A card is only
 * as done as its weakest responsibility; `vagrant` and an all-`removing` card
 * are special cases that win outright.
 */
export function rollupStatus(statuses: Status[]): Status {
  if (statuses.length === 0) return "planned";
  if (statuses.some((s) => s === "vagrant")) return "vagrant";
  if (statuses.every((s) => s === "removing")) return "removing";
  const life = statuses.filter((s) => s !== "removing");
  return life.reduce((worst, s) => (LIFE_RANK[s] < LIFE_RANK[worst] ? s : worst));
}
