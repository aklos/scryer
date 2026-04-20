import { createContext, useContext, useMemo, useState, useCallback, type ReactNode } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  useDraggable,
  useDroppable,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { Trash2, GripVertical, Folder, Plus, CornerUpLeft } from "lucide-react";
import type { C4Node, C4NodeData, Group } from "./types";
import { KIND_ICON } from "./kindIcons";

type DragItem =
  | { kind: "group"; id: string }
  | { kind: "member"; nodeId: string; sourceGroupId: string | null };

interface GroupsDndValue {
  allNodes: C4Node[];
  groups: Group[];
  onUpdateGroups: (fn: (prev: Group[]) => Group[]) => void;
  onNavigateToNode: (id: string) => void;
  parentNode: C4Node | undefined;
  outOfScope: string | null;
  levelChildren: C4Node[];
  visibleGroups: Group[];
  visibleGroupIds: Set<string>;
  nodeById: Map<string, C4Node>;
  nodeToGroup: Map<string, string>;
  childrenOf: Map<string | undefined, Group[]>;
  ungroupedNodes: C4Node[];
  active: DragItem | null;
  patchGroup: (id: string, patch: Partial<Group>) => void;
  deleteGroup: (id: string) => void;
  createEmptyGroup: () => string;
  removeMember: (nodeId: string, sourceGroupId: string) => void;
}

const GroupsDndContext = createContext<GroupsDndValue | null>(null);

function useGroupsDnd(): GroupsDndValue {
  const v = useContext(GroupsDndContext);
  if (!v) throw new Error("GroupsDndContext missing — wrap in <GroupsDndProvider>");
  return v;
}

function scopeMessage(parent: C4Node | undefined): string | null {
  if (!parent) return "Drill into a system or container to manage its groups.";
  const kind = (parent.data as C4NodeData).kind;
  if (kind === "system" || kind === "container") return null;
  return "Groups live at the container and component level. Navigate up to a system or container.";
}

interface GroupsDndProviderProps {
  allNodes: C4Node[];
  groups: Group[];
  onUpdateGroups: (fn: (prev: Group[]) => Group[]) => void;
  currentParentId: string | undefined;
  onNavigateToNode: (id: string) => void;
  children: ReactNode;
}

export function GroupsDndProvider({
  allNodes,
  groups,
  onUpdateGroups,
  currentParentId,
  onNavigateToNode,
  children,
}: GroupsDndProviderProps) {
  const parentNode = currentParentId ? allNodes.find((n) => n.id === currentParentId) : undefined;
  const outOfScope = scopeMessage(parentNode);

  const levelChildren = useMemo(() => {
    if (!parentNode) return [] as C4Node[];
    const parentKind = (parentNode.data as C4NodeData).kind;
    const targetKind = parentKind === "system" ? "container" : parentKind === "container" ? "component" : null;
    if (!targetKind) return [];
    return allNodes.filter((n) => n.parentId === parentNode.id && (n.data as C4NodeData).kind === targetKind);
  }, [allNodes, parentNode]);

  const levelChildIds = useMemo(() => new Set(levelChildren.map((n) => n.id)), [levelChildren]);

  const visibleGroups = useMemo(() => {
    if (!parentNode) return [] as Group[];
    return groups.filter((g) => g.memberIds.length > 0 && g.memberIds.every((id) => levelChildIds.has(id)));
  }, [groups, levelChildIds, parentNode]);

  const visibleGroupIds = useMemo(() => new Set(visibleGroups.map((g) => g.id)), [visibleGroups]);

  const nodeById = useMemo(() => {
    const m = new Map<string, C4Node>();
    for (const n of allNodes) m.set(n.id, n);
    return m;
  }, [allNodes]);

  const nodeToGroup = useMemo(() => {
    const m = new Map<string, string>();
    for (const g of visibleGroups) for (const id of g.memberIds) m.set(id, g.id);
    return m;
  }, [visibleGroups]);

  const childrenOf = useMemo(() => {
    const m = new Map<string | undefined, Group[]>();
    for (const g of visibleGroups) {
      const key = g.parentGroupId && visibleGroupIds.has(g.parentGroupId) ? g.parentGroupId : undefined;
      const list = m.get(key) ?? [];
      list.push(g);
      m.set(key, list);
    }
    return m;
  }, [visibleGroups, visibleGroupIds]);

  const ungroupedNodes = useMemo(
    () => levelChildren.filter((n) => !nodeToGroup.has(n.id)),
    [levelChildren, nodeToGroup],
  );

  const wouldCycle = useCallback((groupId: string, candidateParentId: string): boolean => {
    let cursor: string | undefined = candidateParentId;
    const seen = new Set<string>([groupId]);
    while (cursor) {
      if (seen.has(cursor)) return true;
      seen.add(cursor);
      cursor = groups.find((g) => g.id === cursor)?.parentGroupId;
    }
    return false;
  }, [groups]);

  const patchGroup = useCallback((id: string, patch: Partial<Group>) => {
    onUpdateGroups((prev) => prev.map((g) => (g.id === id ? { ...g, ...patch } : g)));
  }, [onUpdateGroups]);

  const deleteGroup = useCallback((id: string) => {
    onUpdateGroups((prev) => {
      const deleted = prev.find((g) => g.id === id);
      const newParent = deleted?.parentGroupId;
      return prev
        .filter((g) => g.id !== id)
        .map((g) => (g.parentGroupId === id ? { ...g, parentGroupId: newParent } : g));
    });
  }, [onUpdateGroups]);

  const createEmptyGroup = useCallback(() => {
    const id = `group-${crypto.randomUUID()}`;
    onUpdateGroups((prev) => [...prev, { id, name: "New group", memberIds: [] }]);
    return id;
  }, [onUpdateGroups]);

  const moveMember = useCallback((nodeId: string, sourceGroupId: string | null, targetGroupId: string) => {
    if (sourceGroupId === targetGroupId) return;
    onUpdateGroups((prev) =>
      prev.map((g) => {
        if (g.id === sourceGroupId) return { ...g, memberIds: g.memberIds.filter((id) => id !== nodeId) };
        if (g.id === targetGroupId) {
          if (g.memberIds.includes(nodeId)) return g;
          return { ...g, memberIds: [...g.memberIds, nodeId] };
        }
        return g;
      }),
    );
  }, [onUpdateGroups]);

  const removeMember = useCallback((nodeId: string, sourceGroupId: string) => {
    onUpdateGroups((prev) =>
      prev.map((g) => (g.id === sourceGroupId ? { ...g, memberIds: g.memberIds.filter((id) => id !== nodeId) } : g)),
    );
  }, [onUpdateGroups]);

  const [active, setActive] = useState<DragItem | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  const onDragStart = (e: DragStartEvent) => {
    setActive((e.active.data.current as DragItem) ?? null);
  };

  const onDragEnd = (e: DragEndEvent) => {
    const item = (e.active.data.current as DragItem | undefined) ?? null;
    const overId = e.over?.id as string | undefined;
    setActive(null);
    if (!item || !overId) return;

    if (overId === "drop-palette") {
      if (item.kind === "member" && item.sourceGroupId) removeMember(item.nodeId, item.sourceGroupId);
      else if (item.kind === "group") patchGroup(item.id, { parentGroupId: undefined });
      return;
    }

    if (overId === "drop-new-group") {
      const newId = `group-${crypto.randomUUID()}`;
      if (item.kind === "member") {
        onUpdateGroups((prev) => {
          const next = prev.map((g) =>
            g.id === item.sourceGroupId ? { ...g, memberIds: g.memberIds.filter((id) => id !== item.nodeId) } : g,
          );
          return [...next, { id: newId, name: "New group", memberIds: [item.nodeId] }];
        });
      } else if (item.kind === "group") {
        patchGroup(item.id, { parentGroupId: undefined });
      }
      return;
    }

    if (overId.startsWith("group:")) {
      const targetGroupId = overId.slice("group:".length);
      if (item.kind === "member") {
        moveMember(item.nodeId, item.sourceGroupId, targetGroupId);
      } else if (item.kind === "group") {
        if (item.id === targetGroupId) return;
        if (wouldCycle(item.id, targetGroupId)) return;
        patchGroup(item.id, { parentGroupId: targetGroupId });
      }
    }
  };

  const value: GroupsDndValue = {
    allNodes,
    groups,
    onUpdateGroups,
    onNavigateToNode,
    parentNode,
    outOfScope,
    levelChildren,
    visibleGroups,
    visibleGroupIds,
    nodeById,
    nodeToGroup,
    childrenOf,
    ungroupedNodes,
    active,
    patchGroup,
    deleteGroup,
    createEmptyGroup,
    removeMember,
  };

  return (
    <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd}>
      <GroupsDndContext.Provider value={value}>{children}</GroupsDndContext.Provider>
      <DragOverlay>
        {active && <DragGhost item={active} nodeById={nodeById} groups={visibleGroups} />}
      </DragOverlay>
    </DndContext>
  );
}

export function GroupsMain() {
  const {
    parentNode,
    outOfScope,
    childrenOf,
    nodeById,
    active,
    patchGroup,
    deleteGroup,
    createEmptyGroup,
    onNavigateToNode,
    removeMember,
  } = useGroupsDnd();

  if (outOfScope) {
    return (
      <div className="flex-1 flex flex-col overflow-hidden bg-[var(--surface)]">
        <div className="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)] px-8 text-center">
          {outOfScope}
        </div>
      </div>
    );
  }

  const memberLabel = parentNode && (parentNode.data as C4NodeData).kind === "system" ? "containers" : "components";
  const topLevelGroups = childrenOf.get(undefined) ?? [];

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-[var(--surface)]">
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="max-w-3xl mx-auto py-8 px-6 space-y-4">
          <header>
            <h1 className="text-lg font-semibold text-[var(--text)]">Groups</h1>
            <p className="mt-1 text-xs text-[var(--text-tertiary)] leading-relaxed">
              Organize {memberLabel} that share something beyond topology — a deployment unit, a package/folder, an ownership boundary. Drag nodes from the palette into a group. Drag groups onto other groups to nest.
            </p>
          </header>

          <div className="space-y-3">
            {topLevelGroups.map((g) => (
              <GroupCard
                key={g.id}
                group={g}
                childrenOf={childrenOf}
                nodeById={nodeById}
                depth={0}
                active={active}
                onPatch={patchGroup}
                onDelete={deleteGroup}
                onNavigate={onNavigateToNode}
                onRemoveMember={removeMember}
              />
            ))}
            <NewGroupDrop active={active} onCreateEmpty={createEmptyGroup} />
          </div>
        </div>
      </div>
    </div>
  );
}

export function GroupsPalette() {
  const { outOfScope, ungroupedNodes, active, onNavigateToNode } = useGroupsDnd();

  return (
    <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
      <div className="shrink-0 px-3 py-3 border-b border-[var(--border-subtle)]">
        <h2 className="text-[10px] uppercase tracking-wider font-semibold text-[var(--text-muted)]">Available</h2>
        <p className="mt-0.5 text-[11px] text-[var(--text-tertiary)] leading-snug">
          Drag into a group. Drop here to remove from a group.
        </p>
      </div>
      {outOfScope ? (
        <div className="flex-1 flex items-center justify-center text-[11px] text-[var(--text-muted)] italic px-4 text-center">
          {outOfScope}
        </div>
      ) : (
        <PaletteDropArea active={active}>
          {ungroupedNodes.length === 0 ? (
            <div className="text-[11px] text-[var(--text-muted)] italic px-1 py-2">
              Everything at this level is grouped.
            </div>
          ) : (
            <ul className="space-y-1">
              {ungroupedNodes.map((n) => (
                <PaletteItem key={n.id} node={n} onNavigate={onNavigateToNode} />
              ))}
            </ul>
          )}
        </PaletteDropArea>
      )}
    </div>
  );
}

function GroupCard({
  group,
  childrenOf,
  nodeById,
  depth,
  active,
  onPatch,
  onDelete,
  onNavigate,
  onRemoveMember,
}: {
  group: Group;
  childrenOf: Map<string | undefined, Group[]>;
  nodeById: Map<string, C4Node>;
  depth: number;
  active: DragItem | null;
  onPatch: (id: string, patch: Partial<Group>) => void;
  onDelete: (id: string) => void;
  onNavigate: (id: string) => void;
  onRemoveMember: (nodeId: string, sourceGroupId: string) => void;
}) {
  const dragId = `group:${group.id}`;
  const { attributes, listeners, setNodeRef: setDragRef, isDragging } = useDraggable({
    id: `drag-${dragId}`,
    data: { kind: "group", id: group.id } satisfies DragItem,
  });
  const { setNodeRef: setDropRef, isOver } = useDroppable({ id: dragId });

  const children = childrenOf.get(group.id) ?? [];
  const memberNodes = group.memberIds.map((id) => nodeById.get(id)).filter((n): n is C4Node => !!n);

  const showDropCue = !!active && !(active.kind === "group" && active.id === group.id);

  return (
    <div
      ref={setDropRef}
      style={{ marginLeft: depth > 0 ? 16 : 0 }}
      className={`rounded-lg border transition-colors ${
        isOver && showDropCue
          ? "border-[var(--text)] bg-[var(--surface-active)]/40 ring-1 ring-[var(--text)]"
          : "border-[var(--border)] bg-[var(--surface-tint)]/20"
      } ${isDragging ? "opacity-40" : ""}`}
    >
      <div ref={setDragRef} className="group/hdr flex items-start gap-2 p-3 border-b border-[var(--border-subtle)]">
        <button
          type="button"
          className="mt-0.5 text-[var(--text-muted)] hover:text-[var(--text)] cursor-grab active:cursor-grabbing touch-none"
          {...listeners}
          {...attributes}
          aria-label="Drag group"
          title="Drag to nest inside another group"
        >
          <GripVertical size={14} />
        </button>
        <Folder size={14} className="mt-0.5 text-[var(--text-muted)] shrink-0" />
        <div className="flex-1 min-w-0">
          <input
            className="w-full bg-transparent outline-none text-sm font-semibold text-[var(--text)] placeholder-[var(--text-muted)]"
            value={group.name}
            placeholder="Group name…"
            onChange={(e) => onPatch(group.id, { name: e.target.value })}
          />
          <textarea
            ref={(el) => {
              if (el) {
                el.style.height = "auto";
                el.style.height = el.scrollHeight + "px";
              }
            }}
            className="w-full text-xs bg-transparent outline-none text-[var(--text-tertiary)] placeholder-[var(--text-ghost)] resize-none leading-relaxed"
            rows={1}
            value={group.description ?? ""}
            placeholder="What does this group represent?"
            onChange={(e) => {
              const el = e.target;
              el.style.height = "auto";
              el.style.height = el.scrollHeight + "px";
              onPatch(group.id, { description: el.value || undefined });
            }}
          />
        </div>
        {depth > 0 && (
          <button
            type="button"
            className="shrink-0 p-0.5 rounded text-[var(--text-muted)] hover:text-[var(--text)] opacity-0 group-hover/hdr:opacity-100 cursor-pointer transition-opacity"
            title="Move to top level"
            onClick={() => onPatch(group.id, { parentGroupId: undefined })}
          >
            <CornerUpLeft size={12} />
          </button>
        )}
        <button
          type="button"
          className="shrink-0 p-0.5 rounded text-[var(--text-muted)] hover:text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30 opacity-0 group-hover/hdr:opacity-100 cursor-pointer transition-opacity"
          title="Delete group"
          onClick={() => onDelete(group.id)}
        >
          <Trash2 size={12} />
        </button>
      </div>

      <div className="p-2 space-y-1 min-h-[32px]">
        {memberNodes.length === 0 && children.length === 0 && (
          <div className="text-[11px] text-[var(--text-muted)] italic px-2 py-1">
            Drop nodes here.
          </div>
        )}
        {memberNodes.map((n) => (
          <MemberChip
            key={n.id}
            node={n}
            groupId={group.id}
            onNavigate={onNavigate}
            onRemove={() => onRemoveMember(n.id, group.id)}
          />
        ))}
      </div>

      {children.length > 0 && (
        <div className="px-2 pb-2 space-y-2">
          {children.map((c) => (
            <GroupCard
              key={c.id}
              group={c}
              childrenOf={childrenOf}
              nodeById={nodeById}
              depth={depth + 1}
              active={active}
              onPatch={onPatch}
              onDelete={onDelete}
              onNavigate={onNavigate}
              onRemoveMember={onRemoveMember}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function MemberChip({
  node,
  groupId,
  onNavigate,
  onRemove,
}: {
  node: C4Node;
  groupId: string;
  onNavigate: (id: string) => void;
  onRemove: () => void;
}) {
  const data = node.data as C4NodeData;
  const Icon = (KIND_ICON[data.kind] ?? KIND_ICON.operation).Icon;
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: `member:${groupId}:${node.id}`,
    data: { kind: "member", nodeId: node.id, sourceGroupId: groupId } satisfies DragItem,
  });
  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      className={`group/m flex items-center gap-2 px-2 py-1 rounded bg-[var(--surface)] border border-[var(--border-subtle)] hover:border-[var(--border)] cursor-grab active:cursor-grabbing touch-none transition-colors ${
        isDragging ? "opacity-40" : ""
      }`}
    >
      <GripVertical size={12} className="text-[var(--text-muted)] shrink-0" />
      <Icon size={12} className="text-[var(--text-muted)] shrink-0" />
      <button
        type="button"
        className="flex-1 text-left truncate text-sm text-[var(--text-secondary)] hover:text-[var(--text)] cursor-pointer"
        onClick={() => onNavigate(node.id)}
        title="Navigate to node"
      >
        {data.name}
      </button>
      <button
        type="button"
        className="opacity-0 group-hover/m:opacity-100 text-[var(--text-muted)] hover:text-red-500 cursor-pointer transition-opacity text-xs px-1"
        title="Remove from group"
        onClick={onRemove}
      >
        ×
      </button>
    </div>
  );
}

function PaletteItem({ node, onNavigate }: { node: C4Node; onNavigate: (id: string) => void }) {
  const data = node.data as C4NodeData;
  const Icon = (KIND_ICON[data.kind] ?? KIND_ICON.operation).Icon;
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: `palette:${node.id}`,
    data: { kind: "member", nodeId: node.id, sourceGroupId: null } satisfies DragItem,
  });
  return (
    <li
      ref={setNodeRef}
      className={`flex items-center gap-2 px-2 py-1 rounded bg-[var(--surface)] border border-[var(--border-subtle)] hover:border-[var(--border)] cursor-grab active:cursor-grabbing touch-none transition-colors ${
        isDragging ? "opacity-40" : ""
      }`}
      {...listeners}
      {...attributes}
    >
      <GripVertical size={10} className="text-[var(--text-muted)] shrink-0" />
      <Icon size={12} className="text-[var(--text-muted)] shrink-0" />
      <button
        type="button"
        className="flex-1 text-left truncate text-xs text-[var(--text-secondary)] hover:text-[var(--text)] cursor-pointer"
        onClick={() => onNavigate(node.id)}
        title="Navigate to node"
      >
        {data.name}
      </button>
    </li>
  );
}

function PaletteDropArea({
  active,
  children,
}: {
  active: DragItem | null;
  children: React.ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: "drop-palette" });
  const acceptsDrop =
    (active?.kind === "member" && active.sourceGroupId !== null) || active?.kind === "group";
  return (
    <div
      ref={setNodeRef}
      className={`flex-1 overflow-y-auto px-3 py-3 transition-colors ${
        acceptsDrop && isOver ? "bg-[var(--surface-active)]/60" : ""
      }`}
    >
      {children}
      {acceptsDrop && (
        <div
          className={`mt-2 text-[11px] text-center py-2 rounded border border-dashed transition-colors ${
            isOver
              ? "border-[var(--text)] text-[var(--text)]"
              : "border-[var(--border)] text-[var(--text-muted)]"
          }`}
        >
          {active?.kind === "group" ? "Drop to un-nest to top level" : "Drop to remove from group"}
        </div>
      )}
    </div>
  );
}

function NewGroupDrop({
  active,
  onCreateEmpty,
}: {
  active: DragItem | null;
  onCreateEmpty: () => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: "drop-new-group" });
  const acceptsDrop = active?.kind === "member";
  if (active && !acceptsDrop) {
    return null;
  }
  if (!active) {
    return (
      <button
        type="button"
        onClick={onCreateEmpty}
        className="w-full flex items-center justify-center gap-1 text-xs text-[var(--text-muted)] hover:text-[var(--text)] border border-dashed border-[var(--border)] hover:border-[var(--border-strong)] rounded py-2 cursor-pointer transition-colors"
      >
        <Plus size={12} />
        New group
      </button>
    );
  }
  return (
    <div
      ref={setNodeRef}
      className={`text-xs text-center py-2 rounded border border-dashed transition-colors ${
        isOver
          ? "border-[var(--text)] bg-[var(--surface-active)] text-[var(--text)]"
          : "border-[var(--border)] text-[var(--text-muted)]"
      }`}
    >
      + Drop to create a new group
    </div>
  );
}

function DragGhost({
  item,
  nodeById,
  groups,
}: {
  item: DragItem;
  nodeById: Map<string, C4Node>;
  groups: Group[];
}) {
  if (item.kind === "group") {
    const g = groups.find((x) => x.id === item.id);
    return (
      <div className="px-2 py-1 rounded bg-[var(--surface-overlay)] border border-[var(--border)] shadow-md text-sm font-semibold text-[var(--text)] flex items-center gap-2">
        <Folder size={14} className="text-[var(--text-muted)]" />
        {g?.name ?? "Group"}
      </div>
    );
  }
  const n = nodeById.get(item.nodeId);
  const data = n?.data as C4NodeData | undefined;
  const Icon = data ? (KIND_ICON[data.kind] ?? KIND_ICON.operation).Icon : KIND_ICON.operation.Icon;
  return (
    <div className="px-2 py-1 rounded bg-[var(--surface-overlay)] border border-[var(--border)] shadow-md text-sm text-[var(--text-secondary)] flex items-center gap-2">
      <Icon size={12} className="text-[var(--text-muted)]" />
      {data?.name ?? "Node"}
    </div>
  );
}
