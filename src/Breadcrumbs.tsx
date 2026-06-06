/**
 * Top breadcrumb trail — the ancestor chain of the currently open page. For a
 * node it's the parent-id chain from the top-level ancestor down to the node;
 * for a group it's the chain of its containing node plus the group. Clicking a
 * crumb opens that page.
 */

import { ChevronRight, FolderOpen } from "lucide-react";
import type { ScryModel } from "./viewmodel";
import type { Selected } from "./NodePage";

export function Breadcrumbs({
  model,
  selected,
  onSelectNode,
  onSelectGroup,
  projectPath,
}: {
  model: ScryModel;
  selected: Selected | null;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  projectPath: string | null;
}) {
  const byId = (id: string) => model.nodes.find((n) => n.id === id);

  /** Root-first chain of nodes from the top ancestor down to `nodeId`. */
  const nodeChain = (nodeId: string) => {
    const chain: { id: string; name: string }[] = [];
    const seen = new Set<string>();
    let cur = byId(nodeId);
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      chain.unshift({ id: cur.id, name: cur.name });
      cur = cur.parentId ? byId(cur.parentId) : undefined;
    }
    return chain;
  };

  type Crumb = { key: string; label: string; group?: boolean; onClick: () => void };
  const crumbs: Crumb[] = [];

  if (selected?.kind === "node") {
    for (const c of nodeChain(selected.id)) {
      crumbs.push({ key: c.id, label: c.name || "Untitled", onClick: () => onSelectNode(c.id) });
    }
  } else if (selected?.kind === "group") {
    const group = model.groups.find((g) => g.id === selected.id);
    if (group) {
      const container =
        group.parentNodeId ??
        byId(group.memberIds[0] ?? "")?.parentId ??
        null;
      if (container) {
        for (const c of nodeChain(container)) {
          crumbs.push({ key: c.id, label: c.name || "Untitled", onClick: () => onSelectNode(c.id) });
        }
      }
      crumbs.push({
        key: group.id,
        label: group.name || "Group",
        group: true,
        onClick: () => onSelectGroup(group.id),
      });
    }
  }

  return (
    <nav className="flex shrink-0 items-center gap-0.5 overflow-x-auto border-b border-[var(--border)] bg-[var(--surface-overlay)] px-3 py-2 backdrop-blur-md">
      {projectPath && (
        <>
          <span className="truncate max-w-[280px] px-1.5 py-0.5 text-xs text-[var(--text-muted)]">
            {projectPath.replace(/^\/home\/[^/]+/, "~")}
          </span>
          {crumbs.length > 0 && (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
          )}
        </>
      )}
      {crumbs.map((c, i) => {
        const isLast = i === crumbs.length - 1;
        return (
          <span key={c.key} className="flex shrink-0 items-center gap-0.5">
            {i > 0 && (
              <ChevronRight className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
            )}
            <button
              type="button"
              disabled={isLast}
              onClick={c.onClick}
              className={
                isLast
                  ? "flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-semibold text-[var(--text)]"
                  : "flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
              }
            >
              {c.group && <FolderOpen className="h-3 w-3" />}
              {c.label}
            </button>
          </span>
        );
      })}
    </nav>
  );
}
