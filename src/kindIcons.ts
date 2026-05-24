import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { Code, Globe, Package, Puzzle, Table, User } from "lucide-react";
import type { Kind } from "./viewmodel";

const kindColor = "text-[var(--text-muted)]";

export const KIND_ICON: Record<
  Kind,
  { Icon: ComponentType<LucideProps>; color: string; label: string }
> = {
  person:    { Icon: User,    color: kindColor, label: "Person" },
  system:    { Icon: Globe,   color: kindColor, label: "System" },
  container: { Icon: Package, color: kindColor, label: "Container" },
  component: { Icon: Puzzle,  color: kindColor, label: "Component" },
  symbol:    { Icon: Code,    color: kindColor, label: "Symbol" },
  schema:    { Icon: Table,   color: kindColor, label: "Schema" },
};
