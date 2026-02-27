import { useCallback, useMemo, useRef, useState, useEffect } from "react";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  Panel,
  ConnectionMode,
} from "@xyflow/react";
import type {
  DefaultEdgeOptions,
  OnNodesChange,
  OnEdgesChange,
  OnConnect,
  NodeChange,
} from "@xyflow/react";
import { MarkerType } from "@xyflow/react";
import { nodeTypes } from "./nodes";
import { edgeTypes } from "./edges";
import { GuidePanel, ContractPanel } from "./GuidePanels";
import { Bot, Loader2, Trash2, Plus } from "lucide-react";
import { Button } from "./ui";
import type { C4Node, C4Edge, C4Kind, Group, Contract } from "./types";

const defaultEdgeOptions: DefaultEdgeOptions = {
  markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
};
const snapGrid: [number, number] = [20, 20];
const proOptions = { hideAttribution: true };

interface C4CanvasProps {
  expandedPath: string[];
  visibleNodesWithHints: C4Node[];
  visibleEdges: C4Edge[];
  onNodesChange: OnNodesChange;
  onEdgesChange: OnEdgesChange;
  onConnect: OnConnect;
  onBulkDelete: (args: { nodes: C4Node[]; edges: C4Edge[] }) => void;
  onSelectionChange: (args: { nodes: C4Node[]; edges: C4Edge[] }) => void;
  currentModel: string | null;
  currentParentId: string | undefined;
  nodes: C4Node[];
  contract: Contract;
  setContract: React.Dispatch<React.SetStateAction<Contract>>;
  onAutoLayout: () => void | Promise<void>;
  onNewBlankModel: () => void;
  templateList: string[];
  loadTemplate: (name: string) => Promise<void>;
  aiConfigured: boolean;
  aiEnabled: boolean;
  hintLoading: boolean;
  fetchHints: () => void;
  setSettingsOpen: (open: boolean) => void;
  parentName?: string;
  parentKind: C4Kind | undefined;
  selectedNode: C4Node | null;
  selectedEdge: C4Edge | null;
  deleteNode: (id: string) => void;
  setEdges: React.Dispatch<React.SetStateAction<C4Edge[]>>;
  setGroups: React.Dispatch<React.SetStateAction<Group[]>>;
  onAddNode: (kindOverride?: C4Kind, screenPos?: { x: number; y: number }) => void;
  currentParentKind: C4Kind | undefined;
}

export function C4Canvas({
  expandedPath,
  visibleNodesWithHints,
  visibleEdges,
  onNodesChange,
  onEdgesChange,
  onConnect,
  onBulkDelete,
  onSelectionChange,
  currentModel,
  currentParentId,
  nodes,
  contract,
  setContract,
  onAutoLayout,
  onNewBlankModel,
  templateList,
  loadTemplate,
  aiConfigured,
  aiEnabled,
  hintLoading,
  fetchHints,
  setSettingsOpen,
  parentName,
  parentKind,
  selectedNode,
  selectedEdge,
  deleteNode,
  setEdges,
  setGroups,
  onAddNode,
  currentParentKind,
}: C4CanvasProps) {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const selectionStartPos = useRef<{ x: number; y: number } | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; screenX: number; screenY: number } | null>(null);

  // Close context menu on click anywhere or escape
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", onKey); };
  }, [contextMenu]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    // Only trigger on the canvas pane itself, not on nodes, edges, or overlays
    if (!target.closest(".react-flow__pane")) return;
    e.preventDefault();
    if (currentModel === null && nodes.length === 0) return;
    setContextMenu({ x: e.clientX, y: e.clientY, screenX: e.clientX, screenY: e.clientY });
  }, [currentModel, nodes.length]);

  const addFromContext = useCallback((kind?: C4Kind) => {
    if (!contextMenu) return;
    if (kind === "process") {
      window.dispatchEvent(new CustomEvent("add-process", { detail: { componentId: currentParentId } }));
    } else if (kind === "model") {
      window.dispatchEvent(new CustomEvent("add-model", { detail: { componentId: currentParentId } }));
    } else {
      onAddNode(kind, { x: contextMenu.screenX, y: contextMenu.screenY });
    }
    setContextMenu(null);
  }, [contextMenu, currentParentId, onAddNode]);

  // Track the start of a box selection (Shift+drag)
  const handleSelectionStart = useCallback((e: React.MouseEvent) => {
    selectionStartPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  // On selection end, re-evaluate which nodes should be selected using DOM hit testing.
  // ReactFlow's internal getNodesInside has coordinate issues; DOM bounding rects are reliable.
  const handleSelectionEnd = useCallback((e: React.MouseEvent) => {
    const start = selectionStartPos.current;
    selectionStartPos.current = null;
    if (!start) return;

    const selRect = {
      left: Math.min(start.x, e.clientX),
      top: Math.min(start.y, e.clientY),
      right: Math.max(start.x, e.clientX),
      bottom: Math.max(start.y, e.clientY),
    };

    // Skip tiny selections (likely just clicks)
    if (selRect.right - selRect.left < 5 && selRect.bottom - selRect.top < 5) return;

    // Query all node DOM elements and check bounding rect overlap
    const nodeEls = wrapperRef.current?.querySelectorAll('.react-flow__node') ?? [];
    const selectedIds = new Set<string>();
    for (const el of nodeEls) {
      const r = (el as HTMLElement).getBoundingClientRect();
      // Full containment: node must be fully inside selection rect
      if (r.left >= selRect.left && r.right <= selRect.right &&
          r.top >= selRect.top && r.bottom <= selRect.bottom) {
        const id = (el as HTMLElement).dataset.id;
        if (id) selectedIds.add(id);
      }
    }

    // Exclude reference nodes (external systems shown at container level) from box selection
    for (const n of visibleNodesWithHints) {
      if (n.data._reference) selectedIds.delete(n.id);
    }

    // Dispatch corrected selection through onNodesChange
    const changes: NodeChange[] = visibleNodesWithHints.map((n) => ({
      type: "select" as const,
      id: n.id,
      selected: selectedIds.has(n.id),
    }));
    onNodesChange(changes);

    // Node-priority: deselect all edges when nodes are selected
    if (selectedIds.size > 0) {
      setEdges((eds) => eds.map((e) => e.selected ? { ...e, selected: false } : e) as C4Edge[]);
    }
  }, [visibleNodesWithHints, onNodesChange, setEdges]);

  // During box selection, suppress ReactFlow's (incorrect) live select changes.
  // Our handleSelectionEnd dispatches the correct selection at the end.
  const refNodeIds = useMemo(() => new Set(
    visibleNodesWithHints.filter((n) => n.data._reference).map((n) => n.id)
  ), [visibleNodesWithHints]);

  const wrappedOnNodesChange: OnNodesChange = useCallback((changes) => {
    if (selectionStartPos.current) {
      const filtered = changes.filter((c) => c.type !== "select");
      if (filtered.length > 0) onNodesChange(filtered);
      return;
    }
    // Compute resulting selection after applying changes
    const selectionAfter = new Map<string, boolean>();
    for (const n of visibleNodesWithHints) selectionAfter.set(n.id, !!n.selected);
    const changeIds = new Set<string>();
    for (const c of changes) {
      if (c.type === "select") { selectionAfter.set(c.id, c.selected); changeIds.add(c.id); }
    }

    // If multiple nodes would be selected, deselect all reference nodes
    const selectedCount = [...selectionAfter.values()].filter(Boolean).length;
    if (selectedCount > 1) {
      const patched = changes.map((c) =>
        c.type === "select" && refNodeIds.has(c.id) ? { ...c, selected: false } : c
      );
      // Also deselect ref nodes that are already selected but not in this change batch
      const extraDeselects: NodeChange[] = [];
      for (const id of refNodeIds) {
        if (!changeIds.has(id) && selectionAfter.get(id)) {
          extraDeselects.push({ type: "select", id, selected: false });
        }
      }
      onNodesChange([...patched, ...extraDeselects]);
    } else {
      onNodesChange(changes);
    }
  }, [onNodesChange, refNodeIds]);

  const wrappedOnEdgesChange: OnEdgesChange = useCallback((changes) => {
    if (selectionStartPos.current) {
      const filtered = changes.filter((c) => c.type !== "select");
      if (filtered.length > 0) onEdgesChange(filtered);
      return;
    }
    onEdgesChange(changes);
  }, [onEdgesChange]);

  const wrappedOnSelectionChange = useCallback((args: { nodes: C4Node[]; edges: C4Edge[] }) => {
    if (selectionStartPos.current) return;
    onSelectionChange(args);
  }, [onSelectionChange]);

  const onConnectStart = useCallback(() => {
    wrapperRef.current?.classList.add("connecting");
  }, []);

  const onConnectEnd = useCallback(() => {
    wrapperRef.current?.classList.remove("connecting");
  }, []);

  return (
    <div className={`flex-1 relative${parentKind === "component" ? " code-level" : ""}`} ref={wrapperRef} onContextMenu={handleContextMenu}>
      <ReactFlow
        key={expandedPath.join("/")}
        nodes={visibleNodesWithHints}
        edges={visibleEdges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={wrappedOnNodesChange}
        onEdgesChange={wrappedOnEdgesChange}
        onConnect={onConnect}
        onConnectStart={onConnectStart}
        onConnectEnd={onConnectEnd}
        connectionMode={ConnectionMode.Loose}
        defaultEdgeOptions={defaultEdgeOptions}
        edgesReconnectable={false}
        snapToGrid
        snapGrid={snapGrid}
        deleteKeyCode="Delete"
        onDelete={onBulkDelete}
        multiSelectionKeyCode="Shift"
        onSelectionChange={wrappedOnSelectionChange}
        onSelectionStart={handleSelectionStart}
        onSelectionEnd={handleSelectionEnd}
        colorMode="system"
        proOptions={proOptions}
      >
        <Background gap={20} variant={BackgroundVariant.Dots} size={1} color="var(--grid-color, #e4e4e7)" />
        {/* Canvas toolbar */}
        {(currentModel !== null || nodes.length > 0) && (
          <Panel position="top-center" className="!mt-3">
            <div className="flex items-center gap-0.5 rounded-lg border border-zinc-200/80 bg-white/80 backdrop-blur-sm shadow-sm px-1 py-0.5 dark:border-zinc-700/80 dark:bg-zinc-900/80">
              {(selectedNode || selectedEdge) && (
                <>
                  <Button
                    variant="ghost"
                    color="danger"
                    onClick={() => {
                      if (selectedNode) {
                        if (selectedNode.type === "groupBox") setGroups((prev) => prev.filter((g) => g.id !== selectedNode.id));
                        else deleteNode(selectedNode.id);
                      } else if (selectedEdge) {
                        setEdges((eds) => eds.filter((e) => e.id !== selectedEdge.id));
                      }
                    }}
                  >
                    <Trash2 className="h-3 w-3" />
                    {selectedNode?.type === "groupBox" ? "ungroup" : "delete"}
                  </Button>
                </>
              )}
              {(selectedNode || selectedEdge) && <div className="w-px h-4 bg-zinc-200 dark:bg-zinc-700 mx-0.5" />}
              {!currentParentId ? (
                <>
                  <Button variant="ghost" onClick={() => onAddNode("system")}>
                    <Plus className="h-3 w-3" />
                    system
                  </Button>
                  <Button variant="ghost" onClick={() => onAddNode("person")}>
                    <Plus className="h-3 w-3" />
                    person
                  </Button>
                </>
              ) : (
                <Button variant="ghost" onClick={() => onAddNode()}>
                  <Plus className="h-3 w-3" />
                  {currentParentKind === "system" ? "container" : currentParentKind === "container" ? "component" : "operation"}
                </Button>
              )}
              {currentParentKind === "component" && currentParentId && (
                <>
                  <Button
                    variant="ghost"
                    onClick={() => window.dispatchEvent(new CustomEvent("add-process", { detail: { componentId: currentParentId } }))}
                  >
                    <Plus className="h-3 w-3" />
                    process
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => window.dispatchEvent(new CustomEvent("add-model", { detail: { componentId: currentParentId } }))}
                  >
                    <Plus className="h-3 w-3" />
                    model
                  </Button>
                </>
              )}
            </div>
          </Panel>
        )}
        {/* breadcrumbs moved outside ReactFlow to avoid z-index overlap with left panels */}
        <Panel position="bottom-right" className="flex items-center gap-2 !m-2">
          {nodes.length > 0 && (
            <button
              type="button"
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer transition-colors ${
                aiConfigured && aiEnabled
                  ? "text-violet-500 hover:bg-zinc-100 dark:text-violet-400 dark:hover:bg-zinc-800"
                  : aiConfigured
                    ? "text-zinc-400 hover:bg-zinc-100 dark:text-zinc-500 dark:hover:bg-zinc-800"
                    : "text-violet-500 hover:bg-violet-50 dark:text-violet-400 dark:hover:bg-violet-950/30"
              }`}
              onClick={() => aiConfigured && aiEnabled ? (hintLoading || fetchHints()) : setSettingsOpen(true)}
              onContextMenu={(e) => { e.preventDefault(); setSettingsOpen(true); }}
              title={aiConfigured ? "Click to review, right-click for settings" : "Configure AI"}
            >
              {hintLoading && aiConfigured && aiEnabled
                ? <Loader2 className="h-3 w-3 animate-spin" />
                : <Bot className="h-3 w-3" />
              }
              {!aiConfigured ? "Configure AI" : aiEnabled ? "Review" : "AI off"}
            </button>
          )}
          <span className="text-[10px] text-zinc-300 dark:text-zinc-600">scryer <span className="opacity-60">{__APP_VERSION__}</span></span>
        </Panel>
        {(currentModel !== null || nodes.length > 0) && (
          <Controls>
            <button
              type="button"
              title="Auto layout"
              className="react-flow__controls-button"
              onClick={onAutoLayout}
            >
              <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect x="3" y="3" width="7" height="7" /><rect x="14" y="3" width="7" height="7" /><rect x="3" y="14" width="7" height="7" /><rect x="14" y="14" width="7" height="7" />
              </svg>
            </button>
          </Controls>
        )}
      </ReactFlow>
      {/* Welcome intro (no model loaded) */}
      {currentModel === null && nodes.length === 0 && (
        <div className="absolute inset-0 z-10 flex flex-col items-center justify-center pointer-events-none">
          <div className="flex flex-col items-center gap-6 pointer-events-auto max-w-md text-center">
            <div className="flex items-center gap-4">
              <img src="/logo.png" alt="scryer" className="w-16 h-16" />
              <div className="flex flex-col">
                <h1 className="text-5xl font-bold tracking-tight text-zinc-800 dark:text-zinc-100" style={{ fontFamily: "'Space Grotesk Variable', sans-serif" }}>scryer</h1>
                <p className="text-sm text-zinc-400 dark:text-zinc-500">visual planning surface</p>
              </div>
            </div>
            <div className="flex flex-col gap-2 text-sm text-zinc-500 dark:text-zinc-400 leading-relaxed">
              <p>
                Plan your software architecture using the{" "}
                <a href="https://c4model.com" target="_blank" rel="noopener noreferrer" className="font-medium text-blue-500 hover:text-blue-400 underline underline-offset-2">C4 model</a>
                {" "}&mdash; four levels of zoom from systems down to operations. Your AI coding assistant connects via MCP to read and modify the same model.
              </p>
            </div>
            <div className="flex flex-col gap-3 w-full">
              <Button variant="primary" size="md" className="rounded-lg px-4 py-2 text-sm font-medium shadow-sm" onClick={onNewBlankModel}>
                New model
              </Button>
              {templateList.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  <p className="text-[11px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">or start from a template</p>
                  <div className="flex flex-wrap justify-center gap-1.5">
                    {templateList.map((t) => (
                      <Button key={t} variant="secondary" size="md" className="shadow-sm" onClick={() => loadTemplate(t)}>
                        {t.replace(/-/g, " ")}
                      </Button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
      {/* Guide + Contract panel overlay */}
      <div className="absolute top-3 left-3 z-10 flex flex-col gap-2">
        <GuidePanel
          nodes={nodes}
          edges={visibleEdges}
          visibleNodes={visibleNodesWithHints}
          currentParentId={currentParentId}
          parentKind={parentKind}
          parentName={parentName}
        />
        <ContractPanel
          contract={contract}
          onChange={setContract}
          hasNodes={nodes.length > 0}
          isRootLevel={!currentParentId}
        />
      </div>
      {/* Right-click context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[140px] rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80 py-1"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {!currentParentId ? (
            <>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => addFromContext("system")}>
                <Plus className="h-3 w-3" /> Add system
              </button>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => addFromContext("person")}>
                <Plus className="h-3 w-3" /> Add person
              </button>
            </>
          ) : (
            <>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => addFromContext()}>
                <Plus className="h-3 w-3" /> Add {currentParentKind === "system" ? "container" : currentParentKind === "container" ? "component" : "operation"}
              </button>
              {currentParentKind === "component" && (
                <>
                  <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => addFromContext("process")}>
                    <Plus className="h-3 w-3" /> Add process
                  </button>
                  <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => addFromContext("model")}>
                    <Plus className="h-3 w-3" /> Add model
                  </button>
                </>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
