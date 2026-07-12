import { Check } from "lucide-react";
import type { ScryModel, Node } from "../viewmodel";
import type { ModelHealthReport } from "../health";
import { kindIcon } from "../kindIcon";
import { WikiLink } from "../pagekit";
import { ClaimRow, type ClaimRef } from "./NeedsReviewPage";
import { SpecialBody, SpecialHeader } from "./shell";

// --- unmapped claims ------------------------------------------------------------

/** Unmapped claims — the list behind the coverage percentage's complement:
 *  committed leaf claims the model asserts but that read through to no code.
 *  Symmetric with dark code, which is the same blind spot seen from the code's
 *  side.
 *
 *  Computed over the COMMITTED model and its source map — NOT the working draft —
 *  exactly as Rust's `compute_health` is, so the count agrees with the
 *  powerline's "N% claims mapped". (A claim already re-anchored in the pending
 *  plan still shows here until the work folds in; the percentage is committed-
 *  side too, so the two stay consistent.)
 *
 *  Anchorable = a leaf, non-external, non-person node. Each contributes its
 *  responsibilities (keyed by resp id), plus — if it declares any properties —
 *  one data-shape claim keyed by the node id. */
export function findUnmappedClaims(committed: ScryModel | null, planned: ScryModel | null): {
  claims: ClaimRef[];
  shapes: Node[];
} {
  const claims: ClaimRef[] = [];
  const shapes: Node[] = [];
  if (!committed) return { claims, shapes };
  // Leafness spans the AUTHORED tree (committed + plan) — mirrors
  // compute_health: a design-ahead child makes its parent structural, so the
  // parent's claims discharge through the subtree-to-be instead of reading as
  // blind spots.
  const hasChildren = new Set(
    [...committed.nodes, ...(planned?.nodes ?? [])]
      .map((n) => n.parentId)
      .filter(Boolean) as string[],
  );
  const sourceMap = committed.sourceMap ?? {};
  const anchored = (id: string) => (sourceMap[id] ?? []).length > 0;
  for (const node of committed.nodes) {
    if (hasChildren.has(node.id) || node.external || node.kind === "person") continue;
    for (const resp of node.responsibilities ?? []) {
      if (!anchored(resp.id)) claims.push({ node, resp });
    }
    if ((node.properties?.length ?? 0) > 0 && !anchored(node.id)) shapes.push(node);
  }
  return { claims, shapes };
}

export function UnmappedClaimsPage({
  committed,
  model,
  report,
  onSelectNode,
}: {
  committed: ScryModel | null;
  /** The planned layer — leafness spans both, like compute_health. */
  model: ScryModel | null;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const { claims, shapes } = findUnmappedClaims(committed, model);
  const total = claims.length + shapes.length;
  const totals = report?.health.totals;
  const coverage =
    totals && totals.anchorable > 0
      ? Math.round((totals.anchored / totals.anchorable) * 100)
      : null;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Unmapped claims"
        subtitle={
          total === 0
            ? "Every claim reads through to code"
            : `${total} claim${total === 1 ? "" : "s"} that say code exists but anchor to nothing${
                coverage != null ? ` — ${coverage}% mapped` : ""
              }`
        }
      />
      <SpecialBody>
        {total === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              No unmapped claims — every committed claim on a leaf reads through to source.
            </p>
          </div>
        ) : (
          <>
            <p className="mb-4 mt-1 text-2xs text-[var(--text-muted)]">
              These claims say code exists but anchor to nothing — they can't be read through to
              source. Have the agent re-map, or fix the claim. A claim already re-anchored in a
              pending plan clears once the work is folded into the committed model.
            </p>
            <ul className="flex flex-col">
              {claims.map((ref) => (
                <ClaimRow key={ref.resp.id} claim={ref} onSelectNode={onSelectNode} />
              ))}
              {shapes.map((n) => (
                <li
                  key={n.id}
                  className="flex items-center gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0"
                >
                  <WikiLink name={n.name} Icon={kindIcon(n)} onClick={() => onSelectNode(n.id)} />
                  <span className="text-2xs text-[var(--text-ghost)]">data shape</span>
                </li>
              ))}
            </ul>
          </>
        )}
      </SpecialBody>
    </div>
  );
}
