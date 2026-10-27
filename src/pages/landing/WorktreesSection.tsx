/**
 * The landing's "In flight": one line per git worktree saying where its work
 * stands against its base branch — ready to merge, claims left to prove or
 * failing, or merged with nothing of its own left — with its commits ahead
 * and behind and the files it changed. A line opens to the claims still
 * open and a switch to that checkout. A last
 * line names the components more than one worktree changed. Shown only when
 * the project has several checkouts.
 */

import { GitBranch, TriangleAlert } from "lucide-react";
import { PageSection } from "../../shared/ui/pagekit";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import { standing, type WorktreeClaim, type Worktrees } from "../../features/worktrees/useWorktrees";
import { FoldRow } from "./FoldRow";

const TONE: Record<string, string> = { ...CLAIM_TONE, none: "text-[var(--text-muted)]" };
/** Open claims listed per worktree before the rest fold into a count. */
const OPEN_SHOWN = 10;

export function WorktreesSection({
  data,
  onOpenWorktree,
  onOpenClaim,
}: {
  data: Worktrees | null;
  onOpenWorktree: (path: string) => void;
  /** Open a claim of the checkout on screen. */
  onOpenClaim: (c: WorktreeClaim) => void;
}) {
  if (!data || data.worktrees.length < 2) return null;
  const branchOf = (path: string) => data.worktrees.find((w) => w.path === path)?.branch ?? "detached";
  return (
    <PageSection title="In flight" hint="Each worktree's changes against its base branch, and the claims they reach.">
      <ul className="flex flex-col">
        {data.worktrees.map((w) => {
          const s = standing(w);
          const open = w.claims.filter((c) => c.state !== "proven");
          const detail = w.error || open.length > 0 || !w.current;
          return (
            <FoldRow
              key={w.path}
              summary={
                <span data-worktree={w.path} className="flex min-w-0 flex-1 items-baseline gap-2">
                  <GitBranch className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
                  <span className="truncate font-semibold text-[var(--text)]">{w.branch ?? "detached"}</span>
                  {w.current && <span className="shrink-0 text-xs text-[var(--accent)]">open</span>}
                  {s.map((part) => (
                    <span key={part.label} className={`shrink-0 text-xs ${TONE[part.tone]}`}>
                      {part.label}
                    </span>
                  ))}
                  <span className="ml-auto shrink-0 text-xs text-[var(--text-muted)]">
                    {w.base && w.files.length > 0 ? `↑${w.ahead} ↓${w.behind} ${w.base} · ` : ""}
                    {!w.base && w.files.length > 0 ? "uncommitted · " : ""}
                    {w.files.length > 0 && `${w.files.length} file${w.files.length === 1 ? "" : "s"}`}
                  </span>
                </span>
              }
            >
              {detail && (
                <>
                  {w.error && <p className="text-xs text-[var(--text-muted)]">{w.error}</p>}
                  {open.length > 0 && (
                    <ul className="flex flex-col">
                      {open.slice(0, OPEN_SHOWN).map((c) => (
                        <li key={c.respId}>
                          <button
                            type="button"
                            disabled={!w.current}
                            onClick={() => onOpenClaim(c)}
                            title={`${c.file}:${c.line}`}
                            className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-sm enabled:hover:bg-[var(--surface-hover)]"
                          >
                            <span className={`shrink-0 text-xs ${TONE[c.state] ?? ""}`}>{CLAIM_LABEL[c.state]}</span>
                            <span className="min-w-0 flex-1 truncate text-[var(--text)]">{c.statement}</span>
                          </button>
                        </li>
                      ))}
                      {open.length > OPEN_SHOWN && (
                        <li className="px-1 text-xs text-[var(--text-muted)]">and {open.length - OPEN_SHOWN} more</li>
                      )}
                    </ul>
                  )}
                  {!w.current && (
                    <button
                      type="button"
                      onClick={() => onOpenWorktree(w.path)}
                      className="self-start text-xs text-[var(--accent)] hover:underline"
                    >
                      Switch to this worktree
                    </button>
                  )}
                </>
              )}
            </FoldRow>
          );
        })}
        {data.overlaps.length > 0 && (
          <FoldRow
            summary={
              <span className="flex min-w-0 flex-1 items-center gap-2 text-[var(--text-secondary)]">
                <TriangleAlert className="h-3.5 w-3.5 shrink-0 text-amber-600 dark:text-amber-400" />
                {data.overlaps.length} component{data.overlaps.length === 1 ? "" : "s"} changed in more than one worktree
              </span>
            }
          >
            <ul className="flex flex-col gap-0.5">
              {data.overlaps.map((o) => (
                <li key={o.component.id} className="flex items-baseline gap-2 text-sm">
                  <span className="text-[var(--text)]">{o.component.name}</span>
                  <span className="truncate text-xs text-[var(--text-muted)]">{o.worktrees.map(branchOf).join(" · ")}</span>
                </li>
              ))}
            </ul>
          </FoldRow>
        )}
      </ul>
    </PageSection>
  );
}
