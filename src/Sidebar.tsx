import { useEffect, useRef, useState } from "react";
import type { C4Node, C4NodeData, Group, Flow } from "./types";
import { STATUS_COLORS } from "./statusColors";
import { KIND_ICON } from "./kindIcons";
import { Plus, ChevronRight, FileText } from "lucide-react";

interface SidebarProps {
  currentModel: string | null;
  nodes: C4Node[];
  selectedNodeId: string | null;
  expandedPath: string[];
  modelList: string[];
  onLoadModel: (name: string) => void;
  onNavigateToNode: (id: string) => void;
  onExpandNode: (id: string) => void;
  groups: Group[];
  onHighlightGroup: (groupId: string) => void;
  flows: Flow[];
  activeFlowId: string | null;
  onSelectFlow: (id: string) => void;
  onEditFlow: (id: string) => void;
  onNewFlow: () => void;
}

interface TreeContext {
  allNodes: C4Node[];
  groups: Group[];
  selectedNodeId: string | null;
  expandedSet: Set<string>;
  currentGroupIds: Set<string>;
  treeOpen: Set<string>;
  onToggleTree: (id: string) => void;
  onNavigateToNode: (id: string) => void;
  onExpandNode: (id: string) => void;
  onHighlightGroup: (groupId: string) => void;
}

function TreeNode({ node, depth, ctx }: { node: C4Node; depth: number; ctx: TreeContext }) {
  const data = node.data as C4NodeData;
  const children = ctx.allNodes.filter((n) => n.parentId === node.id);
  const hasChildren = children.length > 0;
  const expandable = data.kind === "system" || data.kind === "container" || data.kind === "component";
  const isSelected = node.id === ctx.selectedNodeId;
  const isDrilledInto = ctx.expandedSet.has(node.id);
  const isInCurrentGroup = ctx.currentGroupIds.has(node.id);
  const isOpen = ctx.treeOpen.has(node.id);

  const rowRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (isSelected && rowRef.current) {
      rowRef.current.scrollIntoView({ block: "nearest" });
    }
  }, [isSelected]);

  const kindIcon = KIND_ICON[data.kind] ?? KIND_ICON.operation;
  const isExtOrPerson = data.external || data.kind === "person";

  // Find groups whose members are among this node's children
  const childIds = new Set(children.map((c) => c.id));
  const relevantGroups = ctx.groups.filter((g) => g.memberIds.some((mid) => childIds.has(mid)));
  const groupedChildIds = new Set(relevantGroups.flatMap((g) => g.memberIds.filter((mid) => childIds.has(mid))));
  const ungroupedChildren = children.filter((c) => !groupedChildIds.has(c.id));

  const indent = depth * 12 + 4;

  return (
    <>
      <div
        ref={rowRef}
        className={`flex items-center h-[22px] cursor-pointer text-xs select-none ${
          isSelected
            ? "bg-blue-500/15 text-blue-700 dark:text-blue-300"
            : isInCurrentGroup
              ? "bg-zinc-200/50 text-zinc-700 dark:bg-zinc-700/50 dark:text-zinc-200"
              : isDrilledInto
                ? "text-zinc-800 dark:text-zinc-200"
                : isExtOrPerson
                  ? "text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-700/30"
                  : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-700/30"
        }`}
        style={{ paddingLeft: `${indent}px`, paddingRight: 8 }}
        onClick={() => ctx.onNavigateToNode(node.id)}
        onDoubleClick={(e) => {
          if (expandable) {
            e.preventDefault();
            ctx.onExpandNode(node.id);
          }
        }}
      >
        {hasChildren ? (
          <span
            className="shrink-0 w-4 flex items-center justify-center"
            onClick={(e) => { e.stopPropagation(); ctx.onToggleTree(node.id); }}
          >
            <ChevronRight size={12} className={`text-zinc-500 transition-transform duration-100 ${isOpen ? "rotate-90" : ""}`} />
          </span>
        ) : (
          <span className="w-4 shrink-0" />
        )}
        <kindIcon.Icon className={`shrink-0 mr-1.5 ${kindIcon.color}`} size={14} />
        <span className={`truncate flex-1 ${isDrilledInto ? "font-medium text-zinc-800 dark:text-zinc-200" : ""} ${isExtOrPerson && !isSelected ? "italic" : ""}`}>{data.name}</span>
        {data.status && !data.external && (
          <span className={`w-1.5 h-1.5 rounded-full shrink-0 ml-1 ${STATUS_COLORS[data.status].dotClass}`} />
        )}
      </div>
      {hasChildren && isOpen && (
        <div className="relative">
          <div className="absolute top-0 bottom-2.5 w-px bg-zinc-300/60 dark:bg-zinc-700/40" style={{ left: `${indent + 7}px` }} />
          {relevantGroups.map((g) => {
            const memberNodes = g.memberIds
              .map((mid) => children.find((c) => c.id === mid))
              .filter((c): c is C4Node => c != null);
            if (memberNodes.length === 0) return null;
            const groupOpen = !ctx.treeOpen.has(g.id);
            return (
              <GroupTreeRow key={g.id} group={g} memberNodes={memberNodes} depth={depth + 1} open={groupOpen} ctx={ctx} />
            );
          })}
          {ungroupedChildren.map((child) => (
            <TreeNode key={child.id} node={child} depth={depth + 1} ctx={ctx} />
          ))}
        </div>
      )}
    </>
  );
}

function GroupTreeRow({ group, memberNodes, depth, open, ctx }: { group: Group; memberNodes: C4Node[]; depth: number; open: boolean; ctx: TreeContext }) {
  const isAtCurrentLevel = memberNodes.some((n) => ctx.currentGroupIds.has(n.id));
  const indent = depth * 12 + 4;
  return (
    <>
      <div
        className={`flex items-center h-[22px] cursor-pointer text-xs ${
          isAtCurrentLevel
            ? "bg-zinc-200/50 text-zinc-600 dark:bg-zinc-700/50 dark:text-zinc-400"
            : "text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-700/30"
        }`}
        style={{ paddingLeft: `${indent}px`, paddingRight: 8 }}
        onClick={() => ctx.onHighlightGroup(group.id)}
      >
        <span
          className="shrink-0 w-4 flex items-center justify-center"
          onClick={(e) => { e.stopPropagation(); ctx.onToggleTree(group.id); }}
        >
          <ChevronRight size={12} className={`text-zinc-500 transition-transform duration-100 ${open ? "rotate-90" : ""}`} />
        </span>
        <span className="truncate flex-1 italic">{group.name}</span>
        <span className="text-[10px] shrink-0 text-zinc-600">{memberNodes.length}</span>
      </div>
      {open && (
        <div className="relative">
          <div className="absolute top-0 bottom-2.5 w-px bg-zinc-300/60 dark:bg-zinc-700/40" style={{ left: `${indent + 7}px` }} />
          {memberNodes.map((child) => (
            <TreeNode key={child.id} node={child} depth={depth + 1} ctx={ctx} />
          ))}
        </div>
      )}
    </>
  );
}

export function Sidebar({
  currentModel, nodes, selectedNodeId, expandedPath, modelList, onLoadModel, onNavigateToNode, onExpandNode,
  groups, onHighlightGroup,
  flows, activeFlowId, onSelectFlow, onEditFlow, onNewFlow,
}: SidebarProps) {
  const [activeTab, setActiveTab] = useState<"nodes" | "flows">("nodes");
  const [treeOpen, setTreeOpen] = useState<Set<string>>(new Set());
  const onToggleTree = (id: string) => {
    setTreeOpen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };
  // Auto-expand tree when drilling into nodes
  useEffect(() => {
    if (expandedPath.length === 0) return;
    setTreeOpen((prev) => {
      const next = new Set(prev);
      let changed = false;
      for (const id of expandedPath) {
        if (!next.has(id)) { next.add(id); changed = true; }
      }
      return changed ? next : prev;
    });
  }, [expandedPath]);

  const rootNodes = nodes.filter((n) => !n.parentId);
  const expandedSet = new Set(expandedPath);
  const currentParentId = expandedPath.length > 0 ? expandedPath[expandedPath.length - 1] : undefined;
  const currentGroupIds = new Set(
    currentParentId ? nodes.filter((n) => n.parentId === currentParentId).map((n) => n.id) : []
  );

  const ctx: TreeContext = {
    allNodes: nodes, groups, selectedNodeId,
    expandedSet, currentGroupIds, treeOpen,
    onToggleTree, onNavigateToNode, onExpandNode, onHighlightGroup,
  };

  const isWelcome = currentModel === null && nodes.length === 0;

  return (
    <aside className="w-60 shrink-0 border-r border-zinc-200 bg-zinc-50 flex flex-col dark:border-zinc-700 dark:bg-zinc-900 relative">
      {isWelcome ? (
        /* Welcome: model list */
        <div className="flex-1 overflow-y-auto">
          {modelList.length > 0 ? (
            <>
              <div className="px-3 pt-3 pb-1.5">
                <span className="text-[10px] uppercase tracking-wider font-semibold text-zinc-400 dark:text-zinc-500">Models</span>
              </div>
              {modelList.map((name) => (
                <div
                  key={name}
                  className="flex items-center gap-2 px-3 py-1.5 cursor-pointer text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800 transition-colors"
                  onClick={() => onLoadModel(name)}
                >
                  <FileText size={14} className="shrink-0 text-zinc-400 dark:text-zinc-500" />
                  <span className="truncate flex-1">{name}</span>
                </div>
              ))}
            </>
          ) : (
            <div className="px-3 py-6 text-xs text-zinc-400 dark:text-zinc-500 text-center">
              No models yet.
            </div>
          )}
        </div>
      ) : (
        /* Normal: tabs + tree */
        <>
          <div className="flex shrink-0 gap-1 px-3 py-1.5 border-b border-zinc-200 dark:border-zinc-700">
            <button
              type="button"
              className={`px-2 py-0.5 text-[10px] uppercase tracking-wider font-semibold cursor-pointer transition-colors rounded ${
                activeTab === "nodes"
                  ? "text-zinc-700 bg-zinc-200 dark:text-zinc-200 dark:bg-zinc-700"
                  : "text-zinc-400 hover:text-zinc-600 hover:bg-zinc-100 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-800"
              }`}
              onClick={() => setActiveTab("nodes")}
            >
              Model
            </button>
            <button
              type="button"
              className={`px-2 py-0.5 text-[10px] uppercase tracking-wider font-semibold cursor-pointer transition-colors rounded ${
                activeTab === "flows"
                  ? "text-zinc-700 bg-zinc-200 dark:text-zinc-200 dark:bg-zinc-700"
                  : "text-zinc-400 hover:text-zinc-600 hover:bg-zinc-100 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-800"
              }`}
              onClick={() => setActiveTab("flows")}
            >
              Flows
            </button>
          </div>
          <div className="flex-1 overflow-y-auto py-1">
            {activeTab === "nodes" ? (
              <>
                {rootNodes.map((node) => (
                  <TreeNode key={node.id} node={node} depth={0} ctx={ctx} />
                ))}
              </>
            ) : (
              <>
                <div className="flex items-center justify-end px-3 py-1">
                  <button
                    type="button"
                    className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer transition-colors"
                    title="Add flow"
                    onClick={onNewFlow}
                  >
                    <Plus size={12} />
                  </button>
                </div>
                {flows.map((sc) => (
                  <div
                    key={sc.id}
                    className={`flex items-center gap-1.5 py-1 cursor-pointer text-xs transition-colors ${
                      activeFlowId === sc.id
                        ? "bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"
                        : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
                    }`}
                    style={{ paddingLeft: 12, paddingRight: 12 }}
                    onClick={() => onSelectFlow(sc.id)}
                  >
                    <span className="shrink-0 text-zinc-400 dark:text-zinc-500">&#8227;</span>
                    <span className="truncate flex-1">{sc.name}</span>
                    <button
                      type="button"
                      className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer text-xs shrink-0 transition-colors"
                      onClick={(e) => { e.stopPropagation(); onEditFlow(sc.id); }}
                      title="Edit flow"
                    >
                      &#8943;
                    </button>
                  </div>
                ))}
                {flows.length === 0 && (
                  <div className="px-3 py-6 text-xs text-zinc-400 dark:text-zinc-500 text-center">
                    No flows yet.
                    <br />
                    <button
                      type="button"
                      className="mt-1.5 text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 underline cursor-pointer transition-colors"
                      onClick={onNewFlow}
                    >
                      Create one
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </>
      )}
    </aside>
  );
}
