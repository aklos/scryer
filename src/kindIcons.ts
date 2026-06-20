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
};

/**
 * Visual affordance for a symbol that defines a data shape (carries properties,
 * no responsibilities). There is no `schema` kind anymore — the distinction is
 * derived from the node's shape — but a data type still reads better with the
 * table icon than the generic code glyph.
 */
export const DATA_SHAPE_ICON = {
  Icon: Table,
  color: kindColor,
  label: "Data type",
};
