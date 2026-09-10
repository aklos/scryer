/**
 * The node page's "Structural Violations" section — every line the health
 * report charges to this node or anything inside it, in full: the kind, the
 * sentence that says what is wrong and what to do, the file it happens in,
 * and the component charged (a link when it isn't this page). The map and
 * the tree show the count; this is where the count is explained.
 */

import type { ModelHealthReport, StyleViolation } from "../health";
import { rollupStructural, violationKindLabel } from "../health";
import { PageSection } from "../pagekit";
import type { Node, ScryModel } from "../viewmodel";

const KIND_TONE: Record<StyleViolation["kind"], string> = {
  layer_violation: "text-red-600 dark:text-red-400",
  isolation_violation: "text-red-600 dark:text-red-400",
  external_violation: "text-red-600 dark:text-red-400",
  misplaced: "text-red-600 dark:text-red-400",
  cycle: "text-red-600 dark:text-red-400",
  layerless: "text-amber-600 dark:text-amber-400",
  unstyled: "text-[var(--text-muted)]",
};

export function StructuralSection({
  model,
  node,
  report,
  onSelectNode,
}: {
  model: ScryModel;
  node: Node;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const mine = rollupStructural(model, report).get(node.id) ?? [];
  if (mine.length === 0) return null;
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  return (
    <PageSection
      title="Structural Violations"
      count={mine.length}
      hint="Deterministic findings from the code's import graph and the model: imports a declared style forbids, files on the wrong layer's path, import cycles, components with no layer, containers with no style. Fix them in the code; a missing style is the user's call."
    >
      <ul className="flex flex-col divide-y divide-[var(--border-subtle)]">
        {mine.map((v, i) => {
          const charged = byId.get(v.node);
          const other = v.other ? byId.get(v.other) : undefined;
          return (
            <li key={`${v.kind}:${v.node}:${v.file}:${v.other ?? ""}:${i}`} className="flex flex-col gap-0.5 py-2 text-sm">
              <div className="flex items-center gap-2">
                <span className={`shrink-0 text-[10px] uppercase tracking-wider ${KIND_TONE[v.kind]}`}>
                  {violationKindLabel(v.kind)}
                </span>
                {charged && charged.id !== node.id && (
                  <button
                    type="button"
                    className="truncate text-xs text-[var(--text-secondary)] hover:underline"
                    onClick={() => onSelectNode(charged.id)}
                  >
                    {charged.name}
                  </button>
                )}
                {other && (
                  <>
                    <span className="text-[var(--text-ghost)]">→</span>
                    <button
                      type="button"
                      className="truncate text-xs text-[var(--text-secondary)] hover:underline"
                      onClick={() => onSelectNode(other.id)}
                    >
                      {other.name}
                    </button>
                  </>
                )}
              </div>
              <p className="text-[var(--text)]">{v.detail}</p>
              {v.file && <p className="font-mono text-xs text-[var(--text-muted)]">{v.file}</p>}
            </li>
          );
        })}
      </ul>
    </PageSection>
  );
}
