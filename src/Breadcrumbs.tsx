/**
 * Top breadcrumb trail. Path is a stack of node ids descended into (or [] at
 * the root); clicking a crumb navigates back up.
 */

import { ChevronRight } from "lucide-react";
import type { ScryModel } from "./viewmodel";

export function Breadcrumbs({
  model,
  path,
  onJump,
}: {
  model: ScryModel;
  /** Stack of node ids, root-most first. `[]` is the root surface. */
  path: string[];
  /** Navigate to depth `index + 1` (use -1 to return to the root). */
  onJump: (index: number) => void;
}) {
  return (
    <nav className="flex shrink-0 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--surface-overlay)] px-3 py-2 backdrop-blur-md">
      <Crumb
        label="System"
        active={path.length === 0}
        onClick={() => onJump(-1)}
      />
      {path.map((nodeId, i) => {
        const isLast = i === path.length - 1;
        const label = model.nodes.find((n) => n.id === nodeId)?.name ?? nodeId;
        return (
          <span key={nodeId} className="flex items-center gap-0.5">
            <ChevronRight className="h-3.5 w-3.5 text-[var(--text-ghost)]" />
            <Crumb
              label={label}
              active={isLast}
              onClick={() => onJump(i)}
            />
          </span>
        );
      })}
    </nav>
  );
}

function Crumb({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={active}
      onClick={onClick}
      className={
        active
          ? "rounded px-1.5 py-0.5 text-xs font-semibold text-[var(--text)]"
          : "rounded px-1.5 py-0.5 text-xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
      }
    >
      {label}
    </button>
  );
}
