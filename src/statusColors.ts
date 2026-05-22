/**
 * Status model — aligned with the v0.3 schema in `crates/scryer-core/src/lib.rs`.
 *
 *  proposed    — planned, no code yet
 *  implemented — code exists; may still be incomplete
 *  verified    — responsibilities checked against code
 *  vagrant     — exists but no upstream responsibility justifies it
 */

export type Status = "proposed" | "implemented" | "verified" | "changed" | "vagrant" | "relocated";

export const STATUS_COLORS: Record<
  Status,
  { icon: string; dot: string; badge: string; label: string }
> = {
  proposed: {
    icon: "text-blue-500 dark:text-blue-400",
    dot: "bg-blue-500 dark:bg-blue-400",
    badge: "text-blue-950 dark:text-blue-950",
    label: "Proposed",
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
  changed: {
    icon: "text-orange-500 dark:text-orange-400",
    dot: "bg-orange-500 dark:bg-orange-400",
    badge: "text-orange-950 dark:text-orange-950",
    label: "Changed",
  },
  vagrant: {
    icon: "text-red-500 dark:text-red-400",
    dot: "bg-red-500 dark:bg-red-400",
    badge: "text-red-950 dark:text-red-950",
    label: "Vagrant",
  },
  relocated: {
    icon: "text-violet-500 dark:text-violet-400",
    dot: "bg-violet-500 dark:bg-violet-400",
    badge: "text-violet-950 dark:text-violet-950",
    label: "Relocated",
  },
};

const LIFE_RANK: Record<Status, number> = {
  proposed: 0,
  relocated: 1,
  changed: 2,
  implemented: 3,
  verified: 4,
  vagrant: 5,
};

/**
 * Roll a set of responsibility statuses up to one node status. A node is only
 * as done as its weakest responsibility; `vagrant` wins outright.
 */
export function rollupStatus(statuses: Status[]): Status {
  if (statuses.length === 0) return "proposed";
  if (statuses.some((s) => s === "vagrant")) return "vagrant";
  if (statuses.some((s) => s === "relocated")) return "relocated";
  if (statuses.some((s) => s === "changed")) return "changed";
  return statuses.reduce((worst, s) =>
    LIFE_RANK[s] < LIFE_RANK[worst] ? s : worst,
  );
}
