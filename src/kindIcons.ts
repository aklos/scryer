import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { User, Globe, Package, Puzzle, Code, Workflow, Table } from "lucide-react";
import type { C4Kind } from "./types";

const kindColor = "text-[var(--text-muted)]";

export const KIND_ICON: Record<C4Kind, { Icon: ComponentType<LucideProps>; color: string; label: string }> = {
  person:    { Icon: User,     color: kindColor, label: "Person" },
  system:    { Icon: Globe,    color: kindColor, label: "System" },
  container: { Icon: Package,  color: kindColor, label: "Container" },
  component: { Icon: Puzzle,   color: kindColor, label: "Component" },
  operation: { Icon: Code,     color: kindColor, label: "Operation" },
  process:   { Icon: Workflow, color: kindColor, label: "Process" },
  model:     { Icon: Table,    color: kindColor, label: "Model" },
};
