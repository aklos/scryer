import type { Status } from "./types";

export const STATUS_COLORS: Record<Status, {
  strokeClass: string;
  dimStrokeClass: string;
  hex: string;
  label: string;
  dotClass: string;
}> = {
  implemented: {
    strokeClass: "stroke-emerald-500 dark:stroke-emerald-400",
    dimStrokeClass: "stroke-emerald-500/70 dark:stroke-emerald-400/40",
    hex: "#10b981",
    label: "Implemented",
    dotClass: "bg-emerald-500 dark:bg-emerald-400",
  },
  proposed: {
    strokeClass: "stroke-blue-500 dark:stroke-blue-400",
    dimStrokeClass: "stroke-blue-500/70 dark:stroke-blue-400/40",
    hex: "#3b82f6",
    label: "Proposed",
    dotClass: "bg-blue-500 dark:bg-blue-400",
  },
  changed: {
    strokeClass: "stroke-amber-500 dark:stroke-amber-400",
    dimStrokeClass: "stroke-amber-500/70 dark:stroke-amber-400/40",
    hex: "#f59e0b",
    label: "Changed",
    dotClass: "bg-amber-500 dark:bg-amber-400",
  },
  deprecated: {
    strokeClass: "stroke-red-500 dark:stroke-red-400",
    dimStrokeClass: "stroke-red-500/70 dark:stroke-red-400/40",
    hex: "#ef4444",
    label: "Deprecated",
    dotClass: "bg-red-500 dark:bg-red-400",
  },
};
