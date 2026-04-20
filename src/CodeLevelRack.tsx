import { useEffect, useMemo, useRef, useState, useCallback } from "react";
import { Code, Workflow, Table, Plus, Trash2, ChevronRight, ChevronDown } from "lucide-react";
import { DescriptionText, type MentionNodeInfo } from "./DescriptionText";
import { STATUS_COLORS } from "./statusColors";
import { Button } from "./ui";
import type {
  C4Node,
  C4NodeData,
  C4Kind,
  Status,
  ModelProperty,
} from "./types";

interface CodeLevelRackProps {
  nodes: C4Node[];
  onSelectNode: (id: string) => void;
  selectedNodeId: string | null;
  currentParentId: string;
  onAddNode: (kind?: C4Kind) => void;
  onDeleteNode: (id: string) => void;
}

const COLUMN_CONFIG = [
  { kind: "model" as const, title: "Models", icon: Table },
  { kind: "operation" as const, title: "Operations", icon: Code },
  { kind: "process" as const, title: "Processes", icon: Workflow },
] as const;

function StatusDot({ status }: { status?: Status }) {
  if (!status) return null;
  const sc = STATUS_COLORS[status];
  return (
    <span
      className={`inline-block w-2 h-2 rounded-full shrink-0 ${sc.dotClass}`}
      title={sc.label}
    />
  );
}

function PropertyList({ properties }: { properties: ModelProperty[] }) {
  if (properties.length === 0) return null;
  return (
    <div className="mt-2 space-y-0.5">
      {properties.map((p) => (
        <div key={p.label} className="flex items-baseline gap-2 text-[11px] leading-snug">
          <span className="font-mono font-medium text-[var(--text-secondary)] shrink-0">.{p.label}</span>
          {p.description && (
            <span className="text-[var(--text-muted)] truncate">{p.description}</span>
          )}
        </div>
      ))}
    </div>
  );
}

function RackCard({
  node,
  selected,
  onClick,
  onDelete,
  nodeMap,
}: {
  node: C4Node;
  selected: boolean;
  onClick: () => void;
  onDelete: () => void;
  nodeMap: Map<string, MentionNodeInfo>;
}) {
  const data = node.data as C4NodeData;
  return (
    <button
      type="button"
      className={`w-full text-left rounded-lg border px-3 py-2.5 transition-colors cursor-pointer group ${
        selected
          ? "border-[var(--text)] border-2 px-[11px] py-[9px]"
          : "border-[var(--border)] bg-[var(--surface-raised)] hover:border-[var(--border-strong)]"
      }`}
      onClick={onClick}
    >
      <div className="flex items-center gap-1.5">
        <StatusDot status={data.status} />
        <span className="font-mono text-sm font-semibold text-[var(--text)] truncate">
          {data.name}
        </span>
        <button
          type="button"
          className="ml-auto opacity-0 group-hover:opacity-100 transition-opacity p-0.5 rounded hover:bg-red-100 dark:hover:bg-red-900/30 text-[var(--text-muted)] hover:text-red-500 cursor-pointer"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          title="Delete"
        >
          <Trash2 className="h-3 w-3" />
        </button>
      </div>
      {data.description && (
        <div className="mt-1 text-sm leading-relaxed text-[var(--text-tertiary)]">
          <DescriptionText text={data.description} nodeMap={nodeMap} />
        </div>
      )}
      {data.kind === "model" && data.properties && (
        <PropertyList properties={data.properties} />
      )}
    </button>
  );
}

function RackColumn({
  title,
  kind,
  icon: Icon,
  nodes,
  selectedNodeId,
  onSelectNode,
  onAddNode,
  onDeleteNode,
  nodeMap,
  stacked,
}: {
  title: string;
  kind: C4Kind;
  icon: React.ComponentType<{ className?: string }>;
  nodes: C4Node[];
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
  onAddNode: (kind: C4Kind) => void;
  onDeleteNode: (id: string) => void;
  nodeMap: Map<string, MentionNodeInfo>;
  stacked: boolean;
}) {
  const sorted = useMemo(
    () =>
      [...nodes].sort((a, b) =>
        (a.data as C4NodeData).name.localeCompare((b.data as C4NodeData).name),
      ),
    [nodes],
  );

  const [collapsed, setCollapsed] = useState(stacked && nodes.length === 0);

  // Auto-expand when items appear, auto-collapse empty sections on stack
  useEffect(() => {
    if (!stacked) return;
    if (nodes.length > 0 && collapsed) setCollapsed(false);
  }, [nodes.length, stacked]);

  const expanded = !stacked || !collapsed;
  const Chevron = expanded ? ChevronDown : ChevronRight;

  return (
    <div className={`flex flex-col min-w-0 min-h-0 ${
      stacked
        ? `border-b last:border-b-0 border-[var(--border)]${expanded ? " flex-1" : ""}`
        : "flex-1 border-r last:border-r-0 border-[var(--border)]"
    }`}>
      {/* Column header */}
      <div
        className={`flex items-center gap-1.5 px-3 py-2 h-[32px] border-b border-[var(--border)] bg-[var(--surface-overlay)] shrink-0 ${
          stacked ? "cursor-pointer select-none" : ""
        }`}
        onClick={stacked ? () => setCollapsed((c) => !c) : undefined}
      >
        {stacked && <Chevron className="h-3 w-3 text-[var(--text-muted)]" />}
        <Icon className="h-3.5 w-3.5 text-[var(--text-muted)]" />
        <span className="text-xs font-medium text-[var(--text-secondary)]">
          {title}
        </span>
        <span className="text-[10px] text-[var(--text-muted)]">
          {sorted.length}
        </span>
        <Button
          variant="ghost"
          className="ml-auto !px-1.5 !py-0.5"
          onClick={(e) => { e.stopPropagation(); onAddNode(kind); }}
        >
          <Plus className="h-3 w-3" />
        </Button>
      </div>
      {/* Scrollable card list */}
      {expanded && (
        <div className="flex-1 overflow-y-auto p-2 space-y-1.5 bg-[var(--surface)]">
          {sorted.map((node) => (
            <RackCard
              key={node.id}
              node={node}
              selected={node.id === selectedNodeId}
              onClick={() => onSelectNode(node.id)}
              onDelete={() => onDeleteNode(node.id)}
              nodeMap={nodeMap}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function CodeLevelRack({
  nodes,
  onSelectNode,
  selectedNodeId,
  onAddNode,
  onDeleteNode,
}: CodeLevelRackProps) {
  // Track previous selection so we can pick a neighbor after delete
  const prevSelectedId = useRef(selectedNodeId);
  const prevNodeIds = useRef<string[]>([]);

  // Flat sorted list for neighbor lookup
  const allSorted = useMemo(
    () =>
      [...nodes].sort((a, b) => {
        const ak = (a.data as C4NodeData).kind;
        const bk = (b.data as C4NodeData).kind;
        if (ak !== bk) return ak.localeCompare(bk);
        return (a.data as C4NodeData).name.localeCompare(
          (b.data as C4NodeData).name,
        );
      }),
    [nodes],
  );

  // Auto-select: on entry (nothing selected) or after delete (selected disappeared)
  useEffect(() => {
    if (allSorted.length === 0) return;
    const hasSelection =
      selectedNodeId && allSorted.some((n) => n.id === selectedNodeId);
    if (hasSelection) {
      prevSelectedId.current = selectedNodeId;
      prevNodeIds.current = allSorted.map((n) => n.id);
      return;
    }
    // Selection lost — pick the best neighbor
    const oldIds = prevNodeIds.current;
    const oldSel = prevSelectedId.current;
    if (oldSel && oldIds.length > 0) {
      const oldIdx = oldIds.indexOf(oldSel);
      // Try next, then previous in the old ordering
      for (const candidate of [oldIds[oldIdx + 1], oldIds[oldIdx - 1]]) {
        if (candidate && allSorted.some((n) => n.id === candidate)) {
          onSelectNode(candidate);
          return;
        }
      }
    }
    // Fallback: select first node
    onSelectNode(allSorted[0].id);
  }, [allSorted, selectedNodeId, onSelectNode]);

  // Build a mention node map for @[name] rendering.
  // Includes local siblings AND ref members from neighboring components
  // (harvested from _mentionNames injected by useVisibleNodes).
  const nodeMap = useMemo(() => {
    const map = new Map<string, MentionNodeInfo>();
    for (const n of nodes) {
      const d = n.data as C4NodeData;
      map.set(d.name, { kind: d.kind, status: d.status });
      // Harvest _mentionNames (includes ref component members) for pill rendering
      const mentions = d._mentionNames as
        | { name: string; kind: string; status?: Status }[]
        | undefined;
      if (mentions) {
        for (const m of mentions) {
          if (!map.has(m.name)) {
            map.set(m.name, { kind: m.kind, status: m.status });
          }
        }
      }
    }
    return map;
  }, [nodes]);

  // Partition nodes by kind
  const byKind = useMemo(() => {
    const result: Record<string, C4Node[]> = {
      model: [],
      operation: [],
      process: [],
    };
    for (const n of nodes) {
      const kind = (n.data as C4NodeData).kind;
      if (kind in result) result[kind].push(n);
    }
    return result;
  }, [nodes]);

  const containerRef = useRef<HTMLDivElement>(null);
  const [stacked, setStacked] = useState(false);

  const checkWidth = useCallback(() => {
    if (containerRef.current) {
      setStacked(containerRef.current.clientWidth < 700);
    }
  }, []);

  useEffect(() => {
    checkWidth();
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(checkWidth);
    observer.observe(el);
    return () => observer.disconnect();
  }, [checkWidth]);

  return (
    <div ref={containerRef} className="flex-1 flex flex-col overflow-hidden">
      <div className={`flex-1 overflow-hidden ${stacked ? "flex flex-col" : "flex"}`}>
        {COLUMN_CONFIG.map(({ kind, title, icon }) => (
          <RackColumn
            key={kind}
            title={title}
            kind={kind}
            icon={icon}
            nodes={byKind[kind]}
            selectedNodeId={selectedNodeId}
            onSelectNode={onSelectNode}
            onAddNode={onAddNode}
            onDeleteNode={onDeleteNode}
            nodeMap={nodeMap}
            stacked={stacked}
          />
        ))}
      </div>
    </div>
  );
}
