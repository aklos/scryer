import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { User, Globe, Package, Puzzle, Code, Workflow, Table } from "lucide-react";
import type { C4Kind } from "./types";

export const KIND_ICON: Record<C4Kind, { Icon: ComponentType<LucideProps>; color: string; label: string }> = {
  person:    { Icon: User,     color: "text-zinc-400 dark:text-zinc-500",     label: "Person" },
  system:    { Icon: Globe,    color: "text-slate-400 dark:text-slate-400",   label: "System" },
  container: { Icon: Package,  color: "text-cyan-400 dark:text-cyan-400",     label: "Container" },
  component: { Icon: Puzzle,   color: "text-violet-400 dark:text-violet-400", label: "Component" },
  operation: { Icon: Code,     color: "text-zinc-400 dark:text-zinc-500",     label: "Operation" },
  process:   { Icon: Workflow, color: "text-emerald-400 dark:text-emerald-400", label: "Process" },
  model:     { Icon: Table,    color: "text-amber-400 dark:text-amber-400",   label: "Model" },
};
