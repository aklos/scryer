import type { ChangeItem, ChangeRevision } from "../hooks/useModelStorage";
import { LINK } from "../pagekit";
import { timeLabel } from "./shell";

// --- recent changes -----------------------------------------------------------

const OP_MARK: Record<string, { mark: string; title: string }> = {
  added: { mark: "+", title: "added" },
  changed: { mark: "±", title: "changed" },
  removed: { mark: "−", title: "removed" },
};

/** One revision's diff rows. `context` is hidden when every item shares the
 *  same host (a per-node history, where "on X" would repeat on every line). */
function RevisionItems({
  items,
  showContext,
  onSelectNode,
}: {
  items: readonly ChangeItem[];
  showContext: boolean;
  onSelectNode: (id: string) => void;
}) {
  return (
    <ul className="flex flex-col gap-1">
      {items.map((it, j) => (
        <li key={j} className="flex items-start gap-2">
          <span
            title={OP_MARK[it.op].title}
            className="w-3 shrink-0 pt-px text-center font-mono text-xs text-[var(--text-muted)]"
          >
            {OP_MARK[it.op].mark}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-baseline gap-1.5 text-sm">
              <span className="shrink-0 text-2xs uppercase tracking-[0.07em] text-[var(--text-ghost)]">
                {it.what}
              </span>
              {/* Removals read as struck-through text (never a live link — the
                  thing is gone); additions/changes link to their node. */}
              {it.op !== "removed" && it.nodeId ? (
                <button
                  type="button"
                  onClick={() => onSelectNode(it.nodeId!)}
                  className={`min-w-0 truncate text-left ${LINK}`}
                >
                  {it.label}
                </button>
              ) : (
                <span
                  className={`min-w-0 truncate ${
                    it.op === "removed"
                      ? "text-[var(--text-muted)] line-through decoration-[var(--text-ghost)]"
                      : "text-[var(--text-secondary)]"
                  }`}
                >
                  {it.label}
                </span>
              )}
              {showContext && it.context && (
                <span className="shrink-0 text-2xs text-[var(--text-muted)]">
                  on {it.context}
                </span>
              )}
            </div>
            {it.fields && it.fields.length > 0 && (
              <ul className="mt-0.5 flex flex-col gap-px pl-1">
                {it.fields.map((f) => (
                  <li key={f.field} className="text-2xs leading-relaxed">
                    <span className="text-[var(--text-muted)]">{f.field}: </span>
                    <del className="text-[var(--text-muted)] decoration-[var(--text-ghost)]">
                      {f.from}
                    </del>
                    <span className="text-[var(--text-ghost)]"> → </span>
                    <span className="text-[var(--text-secondary)]">{f.to}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

/** Shared revision feed: time + attribution header per edit burst, then its
 *  diff rows. Drives both the global Recent-changes page and a single node's
 *  History tab. */
export function RevisionList({
  revisions,
  showContext = true,
  onSelectNode,
}: {
  revisions: readonly ChangeRevision[];
  /** Hide the "on X" host label — redundant in a per-node history. */
  showContext?: boolean;
  onSelectNode: (id: string) => void;
}) {
  return (
    <div className="flex flex-col">
      {revisions.map((rev, i) => (
        <section
          key={`${rev.at}-${i}`}
          className="border-b border-[var(--border-subtle)] py-3 last:border-b-0"
        >
          <div className="mb-1.5 flex items-center gap-2 font-mono text-2xs tabular-nums text-[var(--text-muted)]">
            {timeLabel(rev.at)}
            {/* Attribution — indigo is the agent's hue. */}
            <span
              className={
                rev.by === "agent"
                  ? "font-sans font-medium text-indigo-600 dark:text-indigo-400"
                  : "font-sans font-medium text-[var(--text-tertiary)]"
              }
            >
              {rev.by === "agent" ? "agent" : "you"}
            </span>
            <span className="text-[var(--text-ghost)]">
              {rev.items.length} change{rev.items.length === 1 ? "" : "s"}
            </span>
          </div>
          <RevisionItems items={rev.items} showContext={showContext} onSelectNode={onSelectNode} />
        </section>
      ))}
    </div>
  );
}
