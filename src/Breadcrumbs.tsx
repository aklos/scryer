/**
 * Fixed top breadcrumb trail. Shows the path of surfaces descended into;
 * clicking a crumb navigates back up to that surface.
 */

import { ChevronRight } from "lucide-react";
import type { Model } from "./viewmodel";
import { surfaceTitle } from "./mockModel";

export function Breadcrumbs({
  model,
  path,
  onJump,
}: {
  model: Model;
  /** Stack of surface ids, root first. */
  path: string[];
  /** Navigate to the surface at `index` in the path. */
  onJump: (index: number) => void;
}) {
  return (
    <nav className="flex shrink-0 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--surface-overlay)] px-3 py-2 backdrop-blur-md">
      {path.map((surfaceId, i) => {
        const isLast = i === path.length - 1;
        const label = surfaceTitle(model, surfaceId);
        return (
          <span key={surfaceId} className="flex items-center gap-0.5">
            {i > 0 && (
              <ChevronRight className="h-3.5 w-3.5 text-[var(--text-ghost)]" />
            )}
            <button
              type="button"
              disabled={isLast}
              onClick={() => onJump(i)}
              className={
                isLast
                  ? "rounded px-1.5 py-0.5 text-xs font-semibold text-[var(--text)]"
                  : "rounded px-1.5 py-0.5 text-xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
              }
            >
              {label}
            </button>
          </span>
        );
      })}
    </nav>
  );
}
