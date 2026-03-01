import type { Status } from "./types";

export const STATUS_COLORS: Record<Status, {
  strokeClass: string;
  dimStrokeClass: string;
  hex: string;
  label: string;
  dotClass: string;
  pillClass: string;
  pillHoverClass: string;
}> = {
  implemented: {
    strokeClass: "stroke-emerald-500 dark:stroke-emerald-400",
    dimStrokeClass: "stroke-emerald-500/70 dark:stroke-emerald-400/40",
    hex: "#10b981",
    label: "Implemented",
    dotClass: "bg-emerald-500 dark:bg-emerald-400",
    pillClass: "bg-emerald-100 dark:bg-emerald-900/50 text-emerald-800 dark:text-emerald-200",
    pillHoverClass: "hover:bg-emerald-200 dark:hover:bg-emerald-800/50",
  },
  proposed: {
    strokeClass: "stroke-blue-500 dark:stroke-blue-400",
    dimStrokeClass: "stroke-blue-500/70 dark:stroke-blue-400/40",
    hex: "#3b82f6",
    label: "Proposed",
    dotClass: "bg-blue-500 dark:bg-blue-400",
    pillClass: "bg-blue-100 dark:bg-blue-900/50 text-blue-800 dark:text-blue-200",
    pillHoverClass: "hover:bg-blue-200 dark:hover:bg-blue-800/50",
  },
  changed: {
    strokeClass: "stroke-amber-500 dark:stroke-amber-400",
    dimStrokeClass: "stroke-amber-500/70 dark:stroke-amber-400/40",
    hex: "#f59e0b",
    label: "Changed",
    dotClass: "bg-amber-500 dark:bg-amber-400",
    pillClass: "bg-amber-100 dark:bg-amber-900/50 text-amber-800 dark:text-amber-200",
    pillHoverClass: "hover:bg-amber-200 dark:hover:bg-amber-800/50",
  },
  deprecated: {
    strokeClass: "stroke-red-500 dark:stroke-red-400",
    dimStrokeClass: "stroke-red-500/70 dark:stroke-red-400/40",
    hex: "#ef4444",
    label: "Deprecated",
    dotClass: "bg-red-500 dark:bg-red-400",
    pillClass: "bg-red-100 dark:bg-red-900/50 text-red-800 dark:text-red-200",
    pillHoverClass: "hover:bg-red-200 dark:hover:bg-red-800/50",
  },
};
