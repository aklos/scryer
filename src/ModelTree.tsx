/**
 * Left explorer — the definition surface. An IDE-style tree of the full node
 * hierarchy (persons, systems, containers, components, symbols) with groups
 * rendered as folders that wrap their members. Every node is reachable and
 * first-class. Status dots roll up node health at a glance; agent activity and
 * fresh arrivals are reflected on the rows. Right-click for structural edits
 * (add child, rename, delete, group membership).
 */

import { useState } from "react";
import { ChevronRight, Folder, FolderOpen, Loader2, Plus } from "lucide-react";
import type { ScryModel, Node, Group, Kind } from "./viewmodel";
import { childKindFor } from "./viewmodel";
import type { Editor } from "./editor";
import { effectiveNodeStatus } from "./rollup";
import { rollupStatus } from "./statusColors";
import type { Status } from "./statusColors";
import { STATUS_COLORS } from "./statusColors";
import { kindIcon } from "./kindIcon";
import { KIND_ICON } from "./kindIcons";
import { lookupIcon } from "./IconPicker";
import { InlineText } from "./InlineText";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { ConfirmPopover } from "./ConfirmPopover";
import type { Selected } from "./NodePage";

const INDENT = 14;
const ROW = "group/row flex items-center gap-1 rounded pr-3 h-[26px] cursor-pointer select-none";

// Status dot that stays hidden for healthy (implemented) and unset nodes, so
// the tree only flags rows that actually need attention.
function StatusDotFiltered({
  status,
  className = "",
}: {
  status: Status | null | undefined;
  className?: string;
}) {
  if (!status || status === "implemented") return null;
  const colors = STATUS_COLORS[status];
  return (
    <span
      className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${colors.dot} ${className}`}
      title={colors.label}
    />
  );
}

export function ModelTree({
  model,
  selected,
  expanded,
  onSelectNode,
  onSelectGroup,
  onToggle,
  editor,
  onFill,
  activeNodeIds,
  newNodeIds,
}: {
  model: ScryModel;
  selected: Selected | null;
  expanded: ReadonlySet<string>;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onToggle: (id: string, expand?: boolean) => void;
  editor: Editor | undefined;
  onFill?: (nodeId: string) => void;
  activeNodeIds: ReadonlySet<string>;
  newNodeIds: ReadonlySet<string>;
}) {
  const [width, setWidth] = useState(() => {
    const saved = Number(localStorage.getItem("scryer:treeWidth"));
    return saved >= 220 ? saved : 300;
  });
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirmDel, setConfirmDel] = useState<
    { rect: DOMRect; run: () => void; label: string } | null
  >(null);

  const startResize = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const onMove = (ev: PointerEvent) =>
      setWidth(Math.min(560, Math.max(220, startW + (ev.clientX - startX))));
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setWidth((w) => {
        localStorage.setItem("scryer:treeWidth", String(w));
        return w;
      });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  // --- level derivation -----------------------------------------------------

  const childNodes = (parentId: string | null) =>
    model.nodes.filter((n) => (n.parentId ?? null) === parentId);

  const groupsAtLevel = (parentId: string | null): Group[] =>
    model.groups.filter((g) => {
      if (g.memberIds.length === 0) return (g.parentNodeId ?? null) === parentId;
      return g.memberIds.some(
        (m) => (model.nodes.find((n) => n.id === m)?.parentId ?? null) === parentId,
      );
    });

  const groupOfNode = (nodeId: string) =>
    model.groups.find((g) => g.memberIds.includes(nodeId));

  const descendantCount = (nodeId: string): number => {
    let n = 0;
    const stack = [nodeId];
    while (stack.length) {
      const id = stack.pop()!;
      for (const c of model.nodes) {
        if (c.parentId === id) {
          n += 1;
          stack.push(c.id);
        }
      }
    }
    return n;
  };

  const groupStatus = (g: Group): Status | null => {
    const memberStatuses = g.memberIds
      .map((id) => model.nodes.find((n) => n.id === id))
      .filter((n): n is Node => Boolean(n))
      .map((n) => effectiveNodeStatus(n))
      .filter((s): s is Status => Boolean(s));
    const own = (g.responsibilities ?? []).map((r) => r.status ?? "proposed");
    const all = [...own, ...memberStatuses];
    return all.length ? rollupStatus(all) : null;
  };

  // --- structural edits -----------------------------------------------------

  const addChild = (parent: Node) => {
    if (!editor) return;
    const id = editor.addNode({
      kind: childKindFor(parent.kind),
      parentId: parent.id,
    });
    onToggle(parent.id, true);
    onSelectNode(id);
    setRenaming(id);
  };

  // Top-level nodes have no parent to "add child" from, so they're seeded here.
  const addRoot = (kind: Kind, external: boolean) => {
    if (!editor) return;
    const id = editor.addNode({ kind, external: external || undefined });
    onSelectNode(id);
    setRenaming(id);
  };
  const rootMenuItems = (): ContextMenuItem[] => [
    { id: "sys", label: "Add system", onSelect: () => addRoot("system", false) },
    { id: "person", label: "Add person", onSelect: () => addRoot("person", false) },
    { id: "ext", label: "Add external system", onSelect: () => addRoot("system", true) },
  ];
  const openRootMenu = (e: React.MouseEvent) => {
    if (!editor) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items: rootMenuItems() });
  };

  const nodeMenu = (e: React.MouseEvent, node: Node) => {
    if (!editor) return;
    e.preventDefault();
    e.stopPropagation();
    const items: ContextMenuItem[] = [];
    const canHaveChildren = node.kind !== "symbol" && node.kind !== "person";
    if (canHaveChildren) {
      const childKind = childKindFor(node.kind);
      items.push({
        id: "add",
        label: `Add ${KIND_ICON[childKind].label.toLowerCase()}`,
        onSelect: () => addChild(node),
      });
    }
    if (canHaveChildren && onFill && !node.external) {
      items.push({
        id: "fill",
        label: "Generate children",
        onSelect: () => onFill(node.id),
      });
    }
    items.push({ id: "rename", label: "Rename", onSelect: () => setRenaming(node.id) });

    const level = node.parentId ?? null;
    const current = groupOfNode(node.id);
    if (current) {
      items.push({
        id: "remove-group",
        label: `Remove from ${current.name || "group"}`,
        onSelect: () => editor.setNodeGroup(node.id, null),
      });
    }
    for (const g of groupsAtLevel(level)) {
      if (g.id === current?.id) continue;
      items.push({
        id: `addto-${g.id}`,
        label: `Add to ${g.name || "group"}`,
        onSelect: () => editor.setNodeGroup(node.id, g.id),
      });
    }
    items.push({
      id: "new-group",
      label: "New group from this",
      onSelect: () => {
        const gid = editor.addGroup({ parentNodeId: level, memberIds: [node.id] });
        onSelectGroup(gid);
        setRenaming(gid);
      },
    });
    items.push({
      id: "delete",
      label: "Delete",
      onSelect: () => {
        const kids = descendantCount(node.id);
        setConfirmDel({
          rect: new DOMRect(e.clientX, e.clientY, 0, 0),
          label: `Delete ${node.name || "node"}${
            kids > 0 ? ` and ${kids} descendant${kids === 1 ? "" : "s"}` : ""
          }?`,
          run: () => editor.deleteNode(node.id),
        });
      },
    });
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  const groupMenu = (e: React.MouseEvent, group: Group) => {
    if (!editor) return;
    e.preventDefault();
    e.stopPropagation();
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { id: "rename", label: "Rename", onSelect: () => setRenaming(group.id) },
        {
          id: "delete",
          label: "Delete group",
          onSelect: () =>
            setConfirmDel({
              rect: new DOMRect(e.clientX, e.clientY, 0, 0),
              label: `Delete group ${group.name || ""}? Members are kept.`,
              run: () => editor.deleteGroup(group.id),
            }),
        },
      ],
    });
  };

  // --- rendering ------------------------------------------------------------

  const renderLevel = (parentId: string | null, depth: number): React.ReactNode[] => {
    const groups = groupsAtLevel(parentId);
    const grouped = new Set<string>();
    for (const g of groups)
      for (const m of g.memberIds)
        if ((model.nodes.find((n) => n.id === m)?.parentId ?? null) === parentId)
          grouped.add(m);

    const loose = childNodes(parentId).filter((n) => !grouped.has(n.id));

    const rows: React.ReactNode[] = [];
    for (const n of loose) rows.push(renderNode(n, depth));
    for (const g of groups) rows.push(renderGroup(g, parentId, depth));
    return rows;
  };

  const renderNode = (node: Node, depth: number): React.ReactNode => {
    const hasChildren =
      childNodes(node.id).length > 0 || groupsAtLevel(node.id).length > 0;
    const isOpen = expanded.has(node.id);
    const isSel = selected?.kind === "node" && selected.id === node.id;
    const Icon = lookupIcon(node.icon) ?? kindIcon(node);
    const status = effectiveNodeStatus(node);
    const active = activeNodeIds.has(node.id);
    const fresh = newNodeIds.has(node.id);

    return (
      <div key={node.id}>
        <div
          style={{ paddingLeft: 6 + depth * INDENT }}
          onClick={() => onSelectNode(node.id)}
          onContextMenu={(e) => nodeMenu(e, node)}
          className={`${ROW} ${
            isSel
              ? "bg-[var(--surface-active)] text-[var(--text)]"
              : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
          }`}
        >
          <Chevron has={hasChildren} open={isOpen} onClick={() => onToggle(node.id)} />
          <Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
          <span className="min-w-0 flex-1 truncate text-[12.5px]">
            {renaming === node.id && editor ? (
              <InlineText
                value={node.name}
                autoEdit
                placeholder="name"
                onCommit={(v) => {
                  editor.updateNode(node.id, { name: v });
                  setRenaming(null);
                }}
              />
            ) : (
              <span className={fresh ? "text-amber-600 dark:text-amber-400" : ""}>
                {node.name || <span className="italic text-[var(--text-ghost)]">Untitled</span>}
              </span>
            )}
          </span>
          {active ? (
            <Loader2 className="h-3 w-3 shrink-0 animate-spin text-amber-400" />
          ) : (
            <StatusDotFiltered status={status} className="shrink-0" />
          )}
        </div>
        {isOpen && hasChildren && renderLevel(node.id, depth + 1)}
      </div>
    );
  };

  const renderGroup = (
    group: Group,
    parentId: string | null,
    depth: number,
  ): React.ReactNode => {
    const isOpen = expanded.has(group.id);
    const isSel = selected?.kind === "group" && selected.id === group.id;
    const members = group.memberIds
      .map((id) => model.nodes.find((n) => n.id === id))
      .filter((n): n is Node => n != null && (n.parentId ?? null) === parentId);
    const status = groupStatus(group);
    const FolderIcon = isOpen ? FolderOpen : Folder;

    return (
      <div key={group.id}>
        <div
          style={{ paddingLeft: 4 + depth * INDENT }}
          onClick={() => onSelectGroup(group.id)}
          onContextMenu={(e) => groupMenu(e, group)}
          className={`${ROW} border-l-2 border-amber-400/50 ${
            isSel
              ? "bg-amber-400/10 text-[var(--text)]"
              : "bg-amber-400/[0.04] text-[var(--text-secondary)] hover:bg-amber-400/10"
          }`}
        >
          <Chevron has={members.length > 0} open={isOpen} onClick={() => onToggle(group.id)} />
          <FolderIcon className="h-3.5 w-3.5 shrink-0 text-amber-400" />
          <span className="min-w-0 flex-1 truncate text-[12.5px] italic">
            {renaming === group.id && editor ? (
              <InlineText
                value={group.name}
                autoEdit
                placeholder="group name"
                onCommit={(v) => {
                  editor.updateGroup(group.id, { name: v });
                  setRenaming(null);
                }}
              />
            ) : (
              group.name || <span className="text-[var(--text-ghost)]">Group</span>
            )}
          </span>
          <StatusDotFiltered status={status} className="shrink-0" />
        </div>
        {isOpen &&
          members.map((m) => renderNode(m, depth + 1))}
      </div>
    );
  };

  return (
    <div
      style={{ width }}
      className="relative flex h-full shrink-0 flex-col border-r border-[var(--border)] bg-[var(--surface)]"
    >
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
          Model
        </span>
        {editor && (
          <button
            type="button"
            title="Add top-level node"
            onClick={openRootMenu}
            className="rounded p-0.5 text-[var(--text-ghost)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto pb-4" style={{ scrollbarGutter: "stable" }}>
        {model.nodes.length === 0 ? (
          <div className="flex flex-col items-start gap-3 px-4 py-6 text-[12px] text-[var(--text-ghost)]">
            <span>Empty model. Generate from the codebase, or start one here:</span>
            {editor && (
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => addRoot("system", false)}
                  className="inline-flex items-center gap-1 rounded border border-[var(--border)] px-2 py-1 text-[11px] font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
                >
                  <Plus className="h-3 w-3" /> System
                </button>
                <button
                  type="button"
                  onClick={() => addRoot("person", false)}
                  className="inline-flex items-center gap-1 rounded border border-[var(--border)] px-2 py-1 text-[11px] font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
                >
                  <Plus className="h-3 w-3" /> Person
                </button>
              </div>
            )}
          </div>
        ) : (
          renderLevel(null, 0)
        )}
      </div>

      <div
        onPointerDown={startResize}
        className="group/resize absolute right-0 top-0 z-20 flex h-full w-2 translate-x-1/2 cursor-col-resize items-stretch"
      >
        <span className="m-auto h-full w-px bg-transparent transition-colors group-hover/resize:bg-[var(--color-blue-500)]" />
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menu.items}
          onClose={() => setMenu(null)}
        />
      )}
      {confirmDel && (
        <ConfirmPopover
          anchorRect={confirmDel.rect}
          label={confirmDel.label}
          onConfirm={() => {
            confirmDel.run();
            setConfirmDel(null);
          }}
          onCancel={() => setConfirmDel(null)}
        />
      )}
    </div>
  );
}

function Chevron({
  has,
  open,
  onClick,
}: {
  has: boolean;
  open: boolean;
  onClick: () => void;
}) {
  if (!has) return <span className="h-3.5 w-3.5 shrink-0" />;
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="flex h-3.5 w-3.5 shrink-0 items-center justify-center text-[var(--text-ghost)] hover:text-[var(--text-secondary)] cursor-pointer"
    >
      <ChevronRight
        className={`h-3 w-3 transition-transform ${open ? "rotate-90" : ""}`}
      />
    </button>
  );
}
