import { useCallback, useMemo, useRef, useState, useEffect } from "react";
import { version as appVersion } from "../package.json";
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
import { edgeTypes, StraightEdgesContext } from "./edges";
import { CodeLevelRack } from "./CodeLevelRack";
import { GuidePanel } from "./GuidePanels";
import { Bot, Loader2, Trash2, Plus, Navigation, HelpCircle, Minus } from "lucide-react";
import { Button } from "./ui";
import type { C4Node, C4Edge, C4Kind, Group } from "./types";

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
  onAutoLayout: () => void | Promise<void>;
  onNewBlankModel: () => void;
  onOpenCodebase: () => void;
  templateList: string[];
  loadTemplate: (name: string) => Promise<void>;
  syncing: boolean;
  projectPath?: string;
  onBuildWithAI: () => void;
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
  layoutPending?: boolean;
  setNodes: React.Dispatch<React.SetStateAction<C4Node[]>>;
  followAI: boolean;
  onToggleFollowAI: () => void;
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
  onAutoLayout,
  onNewBlankModel,
  onOpenCodebase,
  templateList,
  loadTemplate,
  syncing,
  projectPath,
  onBuildWithAI,
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
  layoutPending,
  setNodes,
  followAI,
  onToggleFollowAI,
}: C4CanvasProps) {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const selectionStartPos = useRef<{ x: number; y: number } | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; screenX: number; screenY: number; nodeId?: string; edgeId?: string } | null>(null);
  const [showHelp, setShowHelp] = useState(false);
  const [straightEdges, setStraightEdges] = useState(true);

  // Close help popover on escape or click outside
  useEffect(() => {
    if (!showHelp) return;
    const close = () => setShowHelp(false);
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", onKey); };
  }, [showHelp]);

  // Close context menu on click anywhere or escape
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", onKey); };
  }, [contextMenu]);

  // Delete key on selected reference nodes → sever edges
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      // Don't intercept if focus is in an input/textarea
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const selectedRefs = visibleNodesWithHints.filter((n) => n.selected && n.data._reference);
      if (selectedRefs.length === 0) return;
      const refIds = new Set(selectedRefs.map((n) => n.id));
      setEdges((eds) => eds.filter((e) => !refIds.has(e.source) && !refIds.has(e.target)));
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visibleNodesWithHints, setEdges]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    e.preventDefault();
    if (currentModel === null && nodes.length === 0) return;

    // Check if right-click is on a node
    const nodeEl = target.closest(".react-flow__node");
    if (nodeEl) {
      const nodeId = nodeEl.getAttribute("data-id");
      if (nodeId) {
        onNodesChange(visibleNodesWithHints.map((n) => ({
          type: "select" as const,
          id: n.id,
          selected: n.id === nodeId,
        })));
        setContextMenu({ x: e.clientX, y: e.clientY, screenX: e.clientX, screenY: e.clientY, nodeId });
        return;
      }
    }

    // Check if right-click is on an edge
    const edgeEl = target.closest(".react-flow__edge");
    if (edgeEl) {
      const edgeId = edgeEl.getAttribute("data-id");
      if (edgeId) {
        setEdges((eds) => eds.map((ed) => ({ ...ed, selected: ed.id === edgeId })) as C4Edge[]);
        setContextMenu({ x: e.clientX, y: e.clientY, screenX: e.clientX, screenY: e.clientY, edgeId });
        return;
      }
    }

    // Canvas pane right-click — show "add" menu
    if (!target.closest(".react-flow__pane")) return;
    setContextMenu({ x: e.clientX, y: e.clientY, screenX: e.clientX, screenY: e.clientY });
  }, [currentModel, nodes.length, onNodesChange, visibleNodesWithHints]);

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

    // Manually fire onSelectionChange since we suppressed it during drag
    const selectedNodes = visibleNodesWithHints.filter((n) => selectedIds.has(n.id));
    onSelectionChange({ nodes: selectedNodes, edges: [] });
  }, [visibleNodesWithHints, onNodesChange, setEdges, onSelectionChange]);

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


  // Show overlay while layout is pending — hold for min 500ms, then fade out over 200ms
  const [overlayMounted, setOverlayMounted] = useState(!!layoutPending);
  const [overlayOpaque, setOverlayOpaque] = useState(!!layoutPending);
  const overlayShownAt = useRef(0);
  useEffect(() => {
    if (layoutPending) {
      setOverlayMounted(true);
      setOverlayOpaque(true);
      overlayShownAt.current = Date.now();
    } else if (overlayMounted) {
      const elapsed = Date.now() - overlayShownAt.current;
      const holdRemaining = Math.max(0, 500 - elapsed);
      // After hold period, start fade
      const fadeTimer = setTimeout(() => {
        setOverlayOpaque(false);
        // After fade animation, unmount
        setTimeout(() => setOverlayMounted(false), 200);
      }, holdRemaining);
      return () => clearTimeout(fadeTimer);
    }
  }, [layoutPending]);

  // ── Connection proximity: show handles on nodes near cursor during edge drag ──
  const connectSourceRef = useRef<string | null>(null);
  const connectMoveRef = useRef<((e: MouseEvent) => void) | null>(null);

  const handleConnectStart = useCallback((_: unknown, params: { nodeId: string | null }) => {
    connectSourceRef.current = params.nodeId;
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    const PROXIMITY = 80;
    const onMove = (e: MouseEvent) => {
      const nodeEls = wrapper.querySelectorAll('.react-flow__node');
      for (const el of nodeEls) {
        const htmlEl = el as HTMLElement;
        if (htmlEl.dataset.id === connectSourceRef.current) continue;
        const rect = htmlEl.getBoundingClientRect();
        const dx = Math.max(rect.left - e.clientX, 0, e.clientX - rect.right);
        const dy = Math.max(rect.top - e.clientY, 0, e.clientY - rect.bottom);
        htmlEl.classList.toggle('connection-nearby', Math.sqrt(dx * dx + dy * dy) < PROXIMITY);
      }
    };
    connectMoveRef.current = onMove;
    window.addEventListener('mousemove', onMove);
  }, []);

  const handleConnectEnd = useCallback(() => {
    connectSourceRef.current = null;
    if (connectMoveRef.current) {
      window.removeEventListener('mousemove', connectMoveRef.current);
      connectMoveRef.current = null;
    }
    wrapperRef.current?.querySelectorAll('.connection-nearby').forEach(el => el.classList.remove('connection-nearby'));
  }, []);

  const isCodeLevel = parentKind === "component";

  const handleRackSelect = useCallback((id: string) => {
    setNodes((nds) => nds.map((n) => ({ ...n, selected: n.id === id })) as C4Node[]);
  }, [setNodes]);

  const handleRackAdd = useCallback((kind?: C4Kind) => {
    if (kind === "process") {
      window.dispatchEvent(new CustomEvent("add-process", { detail: { componentId: currentParentId } }));
    } else if (kind === "model") {
      window.dispatchEvent(new CustomEvent("add-model", { detail: { componentId: currentParentId } }));
    } else {
      onAddNode(kind);
    }
  }, [currentParentId, onAddNode]);

  if (isCodeLevel) {
    return (
      <div className="flex-1 relative flex flex-col bg-[var(--surface)]">
        <CodeLevelRack
          nodes={visibleNodesWithHints}
          onSelectNode={handleRackSelect}
          selectedNodeId={selectedNode?.id ?? null}
          currentParentId={currentParentId!}
          onAddNode={handleRackAdd}
          onDeleteNode={deleteNode}
        />
        {/* Bottom-right AI + version */}
        <div className="absolute bottom-2 right-2 z-10 flex items-center gap-2">
          {nodes.length > 0 && (
            <button
              type="button"
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer transition-colors ${
                aiConfigured && aiEnabled
                  ? "text-violet-500 hover:bg-[var(--surface-tint)] dark:text-violet-400 dark:hover:bg-[var(--surface-tint)]"
                  : aiConfigured
                    ? "text-[var(--text-muted)] hover:bg-[var(--surface-tint)]"
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
          <span className="text-[10px] text-[var(--text-ghost)]">scryer <span className="opacity-60">{appVersion}</span></span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 relative" ref={wrapperRef} onContextMenu={handleContextMenu}>
      {overlayMounted && (
        <div
          className={`absolute inset-0 z-50 flex items-center justify-center bg-[var(--surface)] transition-opacity duration-200 ${overlayOpaque ? "opacity-100" : "opacity-0"}`}
        >
          <Loader2 className="h-5 w-5 animate-spin text-zinc-400" />
        </div>
      )}
      <StraightEdgesContext.Provider value={straightEdges}>
      <ReactFlow
        key={expandedPath.join("/")}
        nodes={visibleNodesWithHints}
        edges={visibleEdges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={wrappedOnNodesChange}
        onEdgesChange={wrappedOnEdgesChange}
        onConnect={onConnect}
        onConnectStart={handleConnectStart}
        onConnectEnd={handleConnectEnd}
        connectionMode={ConnectionMode.Loose}
        defaultEdgeOptions={defaultEdgeOptions}
        edgesReconnectable={false}
        snapToGrid
        snapGrid={snapGrid}
        deleteKeyCode={["Delete", "Backspace"]}
        onDelete={onBulkDelete}
        multiSelectionKeyCode="Shift"
        onSelectionChange={wrappedOnSelectionChange}
        onSelectionStart={handleSelectionStart}
        onSelectionEnd={handleSelectionEnd}
        colorMode={document.documentElement.classList.contains("dark") ? "dark" : "light"}
        proOptions={proOptions}
      >
        <Background gap={20} variant={BackgroundVariant.Dots} size={1} color="var(--grid-color, #e4e4e7)" />
        {/* Canvas toolbar — hidden during sync */}
        {(currentModel !== null || nodes.length > 0) && !syncing && (
          <Panel position="top-center" className="!mt-3">
            <div className="flex items-center gap-0.5 rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-sm shadow-sm px-1 py-0.5">
              {(selectedNode || selectedEdge) && (
                <>
                  <Button
                    variant="ghost"
                    color="danger"
                    onClick={() => {
                      if (selectedNode) {
                        if (selectedNode.type === "groupBox") setGroups((prev) => prev.filter((g) => g.id !== selectedNode.id));
                        else if (selectedNode.data._reference) setEdges((eds) => eds.filter((e) => e.source !== selectedNode.id && e.target !== selectedNode.id));
                        else deleteNode(selectedNode.id);
                      } else if (selectedEdge) {
                        setEdges((eds) => eds.filter((e) => e.id !== selectedEdge.id));
                      }
                    }}
                  >
                    <Trash2 className="h-3 w-3" />
                    {selectedNode?.type === "groupBox" ? "ungroup" : selectedNode?.data._reference ? "disconnect" : "delete"}
                  </Button>
                </>
              )}
              {(selectedNode || selectedEdge) && <div className="w-px h-4 bg-[var(--surface-active)] mx-0.5" />}
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
            </div>
          </Panel>
        )}
        <Panel position="bottom-right" className="flex items-center gap-2 !m-2">
          {nodes.length > 0 && (
            <button
              type="button"
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer transition-colors ${
                straightEdges
                  ? "text-blue-500 hover:bg-[var(--surface-tint)] dark:text-blue-400 dark:hover:bg-[var(--surface-tint)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--surface-tint)]"
              }`}
              onClick={() => setStraightEdges((v) => !v)}
              title={straightEdges ? "Straight edges (click for curves)" : "Curved edges (click for straight)"}
            >
              <Minus className="h-3 w-3" />
              Straight
            </button>
          )}
          {nodes.length > 0 && (
            <button
              type="button"
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer transition-colors ${
                followAI
                  ? "text-blue-500 hover:bg-[var(--surface-tint)] dark:text-blue-400 dark:hover:bg-[var(--surface-tint)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--surface-tint)]"
              }`}
              onClick={onToggleFollowAI}
              title={followAI ? "Following AI changes (click to disable)" : "Not following AI changes (click to enable)"}
            >
              <Navigation className="h-3 w-3" />
              Follow AI
            </button>
          )}
          {nodes.length > 0 && (
            <button
              type="button"
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer transition-colors ${
                aiConfigured && aiEnabled
                  ? "text-violet-500 hover:bg-[var(--surface-tint)] dark:text-violet-400 dark:hover:bg-[var(--surface-tint)]"
                  : aiConfigured
                    ? "text-[var(--text-muted)] hover:bg-[var(--surface-tint)]"
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
          <div className="relative">
            <button
              type="button"
              className="text-[var(--text-ghost)] hover:text-[var(--text-tertiary)] cursor-pointer transition-colors"
              onClick={() => setShowHelp((v) => !v)}
              title="Keyboard shortcuts"
            >
              <HelpCircle className="h-3 w-3" />
            </button>
            {showHelp && (
              <div
                className="absolute bottom-6 right-0 z-50 w-56 rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] shadow-lg backdrop-blur-sm p-3 text-[11px] text-[var(--text-secondary)] space-y-1.5"
                onPointerDown={(e) => e.stopPropagation()}
              >
                <div className="font-medium text-[var(--text)] mb-2">Shortcuts</div>
                <div className="flex justify-between"><span>Selection box</span><kbd className="text-[10px] bg-[var(--surface-tint)] px-1 rounded">Shift + drag</kbd></div>
                <div className="flex justify-between"><span>Multi-select</span><kbd className="text-[10px] bg-[var(--surface-tint)] px-1 rounded">Shift + click</kbd></div>
                <div className="flex justify-between"><span>Switch model</span><kbd className="text-[10px] bg-[var(--surface-tint)] px-1 rounded">Ctrl+K</kbd></div>
                <div className="flex justify-between"><span>Undo / Redo</span><kbd className="text-[10px] bg-[var(--surface-tint)] px-1 rounded">Ctrl+Z / Y</kbd></div>
                <div className="flex justify-between"><span>Delete selected</span><kbd className="text-[10px] bg-[var(--surface-tint)] px-1 rounded">Del</kbd></div>
                <div className="border-t border-[var(--border-subtle)] pt-1.5 mt-1.5 space-y-1.5">
                  <div className="text-[var(--text-muted)]">Right-click the <b>Review</b> button for AI settings</div>
                </div>
              </div>
            )}
          </div>
          <span className="text-[10px] text-[var(--text-ghost)]">scryer <span className="opacity-60">{appVersion}</span></span>
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
      </StraightEdgesContext.Provider>
      {/* Welcome intro (no model loaded) */}
      {currentModel === null && nodes.length === 0 && (
        <div className="absolute inset-0 z-10 flex flex-col items-center justify-center pointer-events-none">
          <div className="flex flex-col items-center gap-6 pointer-events-auto max-w-md text-center">
            <div className="flex items-center gap-4">
              <img src="/logo.png" alt="scryer" className="w-16 h-16" />
              <div className="flex flex-col">
                <h1 className="text-5xl font-bold tracking-tight text-[var(--text)]" style={{ fontFamily: "'Space Grotesk Variable', sans-serif" }}>scryer</h1>
              </div>
            </div>
            <div className="flex flex-col gap-2 text-sm text-[var(--text-tertiary)] leading-relaxed">
              <p>
                Plan your software architecture using the{" "}
                <a href="https://c4model.com" target="_blank" rel="noopener noreferrer" className="font-medium text-blue-500 hover:text-blue-400 underline underline-offset-2">C4 model</a>
                {" "}&mdash; four levels of zoom from systems down to operations. Your AI coding assistant connects via MCP to read and modify the same model.
              </p>
            </div>
            <div className="flex flex-col gap-3 w-full">
              <Button variant="primary" size="md" className="rounded-lg px-4 py-2 text-sm font-medium shadow-sm" onClick={onOpenCodebase}>
                Open codebase
              </Button>
              <Button variant="secondary" size="md" className="rounded-lg px-4 py-2 text-sm font-medium shadow-sm" onClick={onNewBlankModel}>
                New model
              </Button>
              {templateList.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  <p className="text-[11px] uppercase tracking-wider text-[var(--text-muted)]">or start from a template</p>
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
      {/* Project linked but empty model — compact card, visually distinct from welcome */}
      {currentModel !== null && nodes.length === 0 && projectPath && !syncing && (
        <div className="absolute inset-0 z-10 flex items-center justify-center pointer-events-none">
          <div className="pointer-events-auto w-[380px] rounded-xl border border-[var(--border)] bg-[var(--surface)] shadow-lg overflow-hidden">
            <div className="px-5 py-4 border-b border-[var(--border)] bg-[var(--surface-raised)]">
              <div className="text-xs font-medium uppercase tracking-wider text-[var(--text-muted)] mb-1">New project</div>
              <div className="text-lg font-semibold text-[var(--text)]">{projectPath.split(/[/\\]/).filter(Boolean).pop()}</div>
              <div className="text-[11px] text-[var(--text-muted)] truncate mt-0.5">{projectPath}</div>
            </div>
            <div className="px-5 py-4 flex flex-col gap-2.5">
              <button
                type="button"
                className="flex items-center gap-3 w-full rounded-lg border border-[var(--border)] px-4 py-3 text-left hover:bg-[var(--surface-tint)] cursor-pointer transition-colors"
                onClick={onBuildWithAI}
              >
                <div className="flex-1">
                  <div className="text-sm font-medium text-[var(--text)]">Build with AI</div>
                  <div className="text-[11px] text-[var(--text-muted)] mt-0.5">Scan the codebase and generate an architecture model</div>
                </div>
              </button>
              <button
                type="button"
                className="flex items-center gap-3 w-full rounded-lg border border-[var(--border)] px-4 py-3 text-left hover:bg-[var(--surface-tint)] cursor-pointer transition-colors"
                onClick={onNewBlankModel}
              >
                <div className="flex-1">
                  <div className="text-sm font-medium text-[var(--text)]">Start blank</div>
                  <div className="text-[11px] text-[var(--text-muted)] mt-0.5">Add systems, containers, and components manually</div>
                </div>
              </button>
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
      </div>
      {/* Right-click context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[140px] rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] shadow-sm backdrop-blur-sm py-1"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {contextMenu.nodeId ? (
            <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/30 cursor-pointer transition-colors" onClick={() => {
              const node = visibleNodesWithHints.find((n) => n.id === contextMenu.nodeId);
              if (node) onBulkDelete({ nodes: [node], edges: [] });
              setContextMenu(null);
            }}>
              <Trash2 className="h-3 w-3" /> {visibleNodesWithHints.find((n) => n.id === contextMenu.nodeId)?.data._reference ? "Disconnect" : "Delete"}
            </button>
          ) : contextMenu.edgeId ? (
            <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/30 cursor-pointer transition-colors" onClick={() => {
              const edge = visibleEdges.find((e) => e.id === contextMenu.edgeId);
              if (edge) onBulkDelete({ nodes: [], edges: [edge] });
              setContextMenu(null);
            }}>
              <Trash2 className="h-3 w-3" /> Delete
            </button>
          ) : !currentParentId ? (
            <>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors" onClick={() => addFromContext("system")}>
                <Plus className="h-3 w-3" /> Add system
              </button>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors" onClick={() => addFromContext("person")}>
                <Plus className="h-3 w-3" /> Add person
              </button>
            </>
          ) : (
            <>
              <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors" onClick={() => addFromContext()}>
                <Plus className="h-3 w-3" /> Add {currentParentKind === "system" ? "container" : currentParentKind === "container" ? "component" : "operation"}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
