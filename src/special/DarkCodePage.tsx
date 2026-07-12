import { Check } from "lucide-react";
import type { ScryModel } from "../viewmodel";
import type { ModelHealthReport } from "../health";
import { darkBoundaries } from "../health";
import { kindIcon } from "../kindIcon";
import { WikiLink } from "../pagekit";
import { SpecialBody, SpecialHeader } from "./shell";

// --- dark code ------------------------------------------------------------------

export function DarkCodePage({
  model,
  report,
  onSelectNode,
}: {
  model: ScryModel;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const { groups, total } = darkBoundaries(report);
  const nodeById = new Map(model.nodes.map((n) => [n.id, n] as const));

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Dark code"
        subtitle={
          total === 0
            ? "Every file under a boundary reads through to a claim"
            : `${total} file${total === 1 ? "" : "s"} under a node's boundary that no claim reads into`
        }
      />
      <SpecialBody>
        {total === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              No dark code — every file under a node's boundary is read by some claim.
            </p>
          </div>
        ) : (
          <>
            <p className="mb-4 mt-1 text-2xs text-[var(--text-muted)]">
              These files sit inside a node's boundary, but no claim in its subtree anchors to
              them — the lens can't see them. Most will be boilerplate (generated code, config,
              glue); scan for anything load-bearing the model is missing.
            </p>
            <div className="flex flex-col gap-5">
              {groups.map((g) => {
                const node = nodeById.get(g.nodeId);
                return (
                  <section key={g.nodeId}>
                    <div className="mb-1 flex items-baseline gap-2 border-b border-[var(--border-subtle)] pb-1">
                      <WikiLink
                        name={node?.name ?? g.nodeId}
                        Icon={node ? kindIcon(node) : undefined}
                        onClick={() => onSelectNode(g.nodeId)}
                      />
                      <span className="font-mono text-2xs text-[var(--text-ghost)]">
                        {g.files.length} dark
                      </span>
                    </div>
                    <ul className="flex flex-col gap-px pl-1">
                      {g.files.map((f) => (
                        <li
                          key={f}
                          className="truncate font-mono text-2xs text-[var(--text-muted)]"
                        >
                          {f}
                        </li>
                      ))}
                    </ul>
                  </section>
                );
              })}
            </div>
          </>
        )}
      </SpecialBody>
    </div>
  );
}
