import type { Status } from "./types";
import { getThemedHex } from "./theme";
import type { PaletteRole, Shade } from "./theme";

export const STATUS_COLORS: Record<Status, {
  strokeClass: string;
  dimStrokeClass: string;
  /** Use statusHex() instead for runtime-resolved themed color. */
  hex: string;
  label: string;
  dotClass: string;
  pillClass: string;
  pillHoverClass: string;
}> = {
  proposed: {
    strokeClass: "stroke-blue-500 dark:stroke-blue-400",
    dimStrokeClass: "stroke-blue-500/70 dark:stroke-blue-400/40",
    hex: "#3b82f6",
    label: "Proposed",
    dotClass: "bg-blue-500 dark:bg-blue-400",
    pillClass: "bg-blue-100 dark:bg-blue-900/50 text-blue-800 dark:text-blue-200",
    pillHoverClass: "hover:bg-blue-200 dark:hover:bg-blue-800/50",
  },
  implemented: {
    strokeClass: "stroke-amber-500 dark:stroke-amber-400",
    dimStrokeClass: "stroke-amber-500/70 dark:stroke-amber-400/40",
    hex: "#f59e0b",
    label: "Implemented",
    dotClass: "bg-amber-500 dark:bg-amber-400",
    pillClass: "bg-amber-100 dark:bg-amber-900/50 text-amber-800 dark:text-amber-200",
    pillHoverClass: "hover:bg-amber-200 dark:hover:bg-amber-800/50",
  },
  verified: {
    strokeClass: "stroke-emerald-500 dark:stroke-emerald-400",
    dimStrokeClass: "stroke-emerald-500/70 dark:stroke-emerald-400/40",
    hex: "#10b981",
    label: "Verified",
    dotClass: "bg-emerald-500 dark:bg-emerald-400",
    pillClass: "bg-emerald-100 dark:bg-emerald-900/50 text-emerald-800 dark:text-emerald-200",
    pillHoverClass: "hover:bg-emerald-200 dark:hover:bg-emerald-800/50",
  },
  vagrant: {
    strokeClass: "stroke-violet-500 dark:stroke-violet-400",
    dimStrokeClass: "stroke-violet-500/70 dark:stroke-violet-400/40",
    hex: "#8b5cf6",
    label: "Vagrant",
    dotClass: "bg-violet-500 dark:bg-violet-400",
    pillClass: "bg-violet-100 dark:bg-violet-900/50 text-violet-800 dark:text-violet-200",
    pillHoverClass: "hover:bg-violet-200 dark:hover:bg-violet-800/50",
  },
};

/** Map status → Tailwind color family used in the theme. */
const STATUS_FAMILY: Record<Status, PaletteRole> = {
  proposed: "blue",
  implemented: "amber",
  verified: "emerald",
  vagrant: "violet",
};

/** Get the themed hex color for a status, resolving the current theme palette.
 *  Use this instead of STATUS_COLORS[s].hex for inline styles. */
export function statusHex(status: Status, shade: Shade = "500"): string {
  return getThemedHex(STATUS_FAMILY[status], shade);
}
