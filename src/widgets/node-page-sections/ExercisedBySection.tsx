/**
 * The claims whose tests exercise a symbol. A symbol owns no claims — they
 * belong to its component — but a reader of the symbol wants to know which
 * behaviours run through it: the claims whose test names it and can see it
 * (`exercisedBy`, derived). Each row is the claim, its derived state, and the
 * component that owns it; clicking opens that page on the claim.
 */

import { useMemo } from "react";
import type { ClaimState, DerivedStatus } from "../../entities/model/derived";
import type { ScryModel } from "../../entities/model/viewmodel";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../source-section/SourceSection";
import { StatementText } from "../../entities/model/markup";

const TONE: Record<ClaimState, string> = {
  proven: "text-emerald-700 dark:text-emerald-300",
  planned: "text-sky-700 dark:text-sky-300",
  stale: "text-amber-700 dark:text-amber-300",
  failing: "text-red-700 dark:text-red-300",
};

export function ExercisedBySection({
  claimIds,
  model,
  status,
  onSelectNode,
}: {
  claimIds: string[];
  model: ScryModel;
  status: DerivedStatus | null;
  onSelectNode: (id: string) => void;
}) {
  const rows = useMemo(() => {
    const owner = new Map<string, { nodeId: string; nodeName: string; statement: string }>();
    for (const n of model.nodes)
      for (const r of n.responsibilities ?? []) owner.set(r.id, { nodeId: n.id, nodeName: n.name, statement: r.statement });
    const state = new Map((status?.claims ?? []).map((c) => [c.respId, c.state] as const));
    return claimIds.flatMap((id) => {
      const o = owner.get(id);
      return o ? [{ id, ...o, state: state.get(id) }] : [];
    });
  }, [claimIds, model, status]);
  if (rows.length === 0) return null;
  return (
    <PageSection
      title="Exercised by"
      count={rows.length}
      hint="Claims whose test names this symbol and can reach it. They belong to the component that owns the test."
    >
      <ul className="-mx-1 flex flex-col">
        {rows.map((r) => (
          <li key={r.id}>
            <button
              type="button"
              onClick={() => {
                onSelectNode(r.nodeId);
                window.setTimeout(() => jumpTo(respElementId(r.id)), 250);
              }}
              className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left font-mono text-sm hover:bg-[var(--surface-hover)]"
            >
              {r.state && <span className={`w-14 shrink-0 text-xs ${TONE[r.state]}`}>{r.state}</span>}
              <span className="min-w-0 flex-1 text-[var(--text-secondary)]">
                <StatementText text={r.statement} />
              </span>
              <span className="shrink-0 font-sans text-xs text-[var(--text-ghost)]">{r.nodeName}</span>
            </button>
          </li>
        ))}
      </ul>
    </PageSection>
  );
}
