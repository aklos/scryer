/**
 * The status bar. While an agent owns the model it shows the live activity
 * line with Cancel; idle it shows quiet model stats and two navigation
 * counters — "N to review" (opens the Needs review special page) and the
 * Recent changes journal. No panels live here anymore: cross-cutting surfaces
 * are wiki special pages, the bar just links to them.
 */

import { Flag, History, Loader2, X } from "lucide-react";
import type { AgentSession } from "./hooks/useAgentSession";
import type { ModelBuild } from "./hooks/useModelBuild";
import type { ScryModel } from "./viewmodel";
import type { SpecialPage } from "./NodePage";

interface SyncBarProps {
  model: ScryModel;
  agent: AgentSession;
  build: ModelBuild;
  /** Items awaiting a human verdict (see SpecialPages.buildReviewIndex). */
  reviewCount: number;
  /** Proposed/changed claims the code doesn't discharge yet — the plan size. */
  plannedCount: number;
  onOpenSpecial: (page: SpecialPage) => void;
}

export function SyncBar({
  model,
  agent,
  build,
  reviewCount,
  plannedCount,
  onOpenSpecial,
}: SyncBarProps) {
  const runLabel = build.building
    ? "Building model"
    : build.checking
      ? "Checking for drift"
      : null;
  const busy = agent.running || build.active;

  return (
    <div className="shrink-0 border-t border-[var(--border)] bg-[var(--surface)] select-none">
      {busy && (
        <div className="h-0.5 w-full bg-[var(--border)] overflow-hidden">
          <div className="h-full w-1/3 bg-indigo-500 dark:bg-indigo-400 animate-[shimmer_1.5s_ease-in-out_infinite]" />
        </div>
      )}
      <div className="flex items-center h-7 px-3 gap-3 text-2xs">
        {busy ? (
          <>
            <div className="flex items-center gap-1.5 shrink-0">
              <div className="h-1.5 w-1.5 rounded-full bg-indigo-500 animate-pulse shrink-0" />
              <span className="text-[var(--text-tertiary)] font-medium">Agent</span>
            </div>
            <div className="w-px h-3 bg-[var(--border)]" />
            <div className="flex items-center gap-1.5 text-indigo-600 dark:text-indigo-400 min-w-0">
              <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
              <span className="shrink-0">
                {build.active ? (build.phase ?? runLabel) : agent.label}
              </span>
              <span className="truncate text-[var(--text-muted)]">
                {(build.active ? build.activity : agent.activity)
                  ? `· ${build.active ? build.activity : agent.activity}`
                  : "…"}
              </span>
            </div>
            <div className="flex-1" />
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0"
              onClick={build.active ? build.cancel : agent.cancel}
            >
              <X className="h-3 w-3" />
              Cancel
            </button>
          </>
        ) : (
          <>
            {/* Idle: quiet neutral stats. Chromatic signals are reserved for
                the review counter — the one thing demanding attention. */}
            <span className="text-[var(--text-muted)] shrink-0">
              {model.nodes.length} node{model.nodes.length !== 1 ? "s" : ""}
              {" · "}
              {model.links.length} link{model.links.length !== 1 ? "s" : ""}
              {plannedCount > 0 && ` · ${plannedCount} planned`}
            </span>
            {reviewCount > 0 && (
              <>
                <div className="w-px h-3 bg-[var(--border)]" />
                <button
                  type="button"
                  onClick={() => onOpenSpecial("review")}
                  className="flex items-center gap-1.5 rounded px-2 py-0.5 font-medium text-orange-600 dark:text-orange-400 hover:bg-[var(--surface-hover)] cursor-pointer transition-colors"
                  title="Open Needs review — everything awaiting a human verdict"
                >
                  <Flag className="h-3 w-3" />
                  <span>{reviewCount} to review</span>
                </button>
              </>
            )}
            <div className="flex-1" />
          </>
        )}
        <button
          type="button"
          onClick={() => onOpenSpecial("changes")}
          className="flex items-center rounded p-0.5 text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0"
          aria-label="Recent changes"
          title="Recent changes — the agent's edits this session"
        >
          <History className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
