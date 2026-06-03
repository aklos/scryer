/**
 * App shell.
 *
 * Holds the v0.3 model in storage (project-local on disk, auto-saved) and the
 * navigation path. The current SurfaceView is derived per render from
 * (model, currentNodeId). Mutations come back from Surface as Editor intents.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import { Breadcrumbs } from "./Breadcrumbs";
import { PanZoom } from "./PanZoom";
import { Surface } from "./Surface";
import { ModelContext } from "./modelcontext";
import { ProjectPicker } from "./ProjectPicker";
import { SyncBar } from "./SyncBar";
import { SettingsPanel } from "./SettingsPanel";
import { InspectorPanel } from "./InspectorPanel";
import { useModelStorage } from "./hooks/useModelStorage";
import { useModelBuild, type ModelBuild } from "./hooks/useModelBuild";
import { useAgentSession } from "./hooks/useAgentSession";
import {
  addGroup as addGroupHelper,
  addNode as addNodeHelper,
  addProperty,
  addResponsibility,
  altitudeFor,
  deriveSurfaceView,
  moveResponsibility as moveResponsibilityHelper,
  unlockRelocatedSource as unlockRelocatedSourceHelper,
  removeGroup as removeGroupHelper,
  removeNode as removeNodeHelper,
  removeProperty,
  removeResponsibility,
  updateGroup as updateGroupHelper,
  updateNode as updateNodeHelper,
  updateProperty,
  updateResponsibility,
  type Altitude,
  type Cell,
  type DriftScope,
  type ScryModel,
} from "./viewmodel";
import {
  autoLayout,
  moveGroupInModel,
  placeNodeInModel,
  resizeGroupInModel,
  type Span,
} from "./pack";
import type { Editor } from "./editor";
import type { Selection } from "./selection";

/** Stable empty set so Surface doesn't re-render on a fresh `new Set()` each pass. */
const EMPTY_IDS: ReadonlySet<string> = new Set();

export default function App() {
  useEffect(() => {
    // A dev refresh reloads the webview but not the Rust backend, leaving any
    // in-flight agent session (and its child process) alive and still editing
    // the model. Cancel it on load so a refresh doesn't leave orphans. No-op on
    // a cold start (no session → the error is ignored).
    void invoke("cancel_agent_session").catch(() => {});
  }, []);

  return (
    <ErrorBoundary>
      <ToastProvider>
        <AppBody />
      </ToastProvider>
    </ErrorBoundary>
  );
}

function AppBody() {
  const storage = useModelStorage();
  const build = useModelBuild(storage);
  const model = storage.model;

  if (!model || storage.status !== "ready") {
    return <ProjectPicker storage={storage} build={build} />;
  }
  return (
    <Canvas
      model={model}
      updateModel={storage.updateModel}
      projectPath={storage.projectPath}
      modelRef={storage.modelRef}
      build={build}
      setAgentRunning={storage.setAgentRunning}
      reloadFromDisk={storage.reloadFromDisk}
      newNodeIds={storage.newNodeIds}
      newRespIds={storage.newRespIds}
      clearNewNode={storage.clearNewNode}
      clearNewResp={storage.clearNewResp}
    />
  );
}

function Canvas({
  model,
  updateModel,
  projectPath,
  modelRef: modelRefStr,
  build,
  setAgentRunning,
  reloadFromDisk,
  newNodeIds,
  newRespIds,
  clearNewNode,
  clearNewResp,
}: {
  model: ScryModel;
  updateModel: ReturnType<typeof useModelStorage>["updateModel"];
  projectPath: string | null;
  modelRef: string | null;
  build: ModelBuild;
  setAgentRunning: (running: boolean) => void;
  reloadFromDisk: () => Promise<void>;
  newNodeIds: ReadonlySet<string>;
  newRespIds: ReadonlySet<string>;
  clearNewNode: (id: string) => void;
  clearNewResp: (id: string) => void;
}) {
  const agent = useAgentSession();
  // Per-node fills also own the file while running: suppress canvas write-back
  // so the agent and canvas don't clobber each other's writes.
  useEffect(() => {
    setAgentRunning(agent.running || build.active);
  }, [agent.running, build.active, setAgentRunning]);
  // While a fill runs, the agent owns the file. Poll its writes onto the canvas
  // (the OS file watcher can miss/coalesce events) and load the final result
  // when it ends — never write our stale in-memory model back, which would
  // clobber the agent's work (it would then survive only in the baseline file).
  useEffect(() => {
    if (!agent.running) return;
    const t = setInterval(() => {
      void reloadFromDisk();
    }, 500);
    return () => {
      clearInterval(t);
      void reloadFromDisk();
    };
  }, [agent.running, reloadFromDisk]);
  // Editing is disabled while a build, a drift check, or a per-node fill writes.
  const writing = agent.running || build.active;

  // Cheap, agent-free nudge: which scopes have code changes since the last
  // reconcile. Refreshes on open and whenever an agent run finishes. We keep the
  // full DriftScope list (node + its changed files) so the SyncBar can show
  // *what* drifted, not just a count.
  const [driftScopes, setDriftScopes] = useState<DriftScope[]>([]);
  useEffect(() => {
    if (!projectPath || build.active) return;
    let live = true;
    invoke<DriftScope[]>("get_drift_status", { cwd: projectPath })
      .then((s) => {
        if (live) setDriftScopes(Array.isArray(s) ? s : []);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [projectPath, build.active]);
  const [path, setPath] = useState<string[]>([]);
  const [selection, setSelection] = useState<Selection>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const currentNodeId: string | null =
    path.length > 0 ? path[path.length - 1] : null;
  const surfaceView = useMemo(
    () => deriveSurfaceView(model, currentNodeId),
    [model, currentNodeId],
  );
  const ancestorAltitudes = useMemo((): Altitude[] => {
    if (path.length === 0) return [];
    const alts: Altitude[] = ["system"];
    for (const id of path.slice(0, -1)) {
      const node = model.nodes.find((n) => n.id === id);
      if (node) alts.push(altitudeFor(node.kind));
    }
    return alts;
  }, [model, path]);

  const modelRef = useRef(model);
  modelRef.current = model;

  const handleNavigate = useCallback((nodeId: string) => {
    const node = modelRef.current.nodes.find((n) => n.id === nodeId);
    if (!node) return;
    if (node.kind === "symbol" || node.kind === "person") return;
    setSelection(null);
    setPath((p) => (p[p.length - 1] === nodeId ? p : [...p, nodeId]));
  }, []);

  const handleJump = useCallback((index: number) => {
    setSelection(null);
    setPath((p) => (index < 0 ? [] : p.slice(0, index + 1)));
  }, []);

  const selectNode = useCallback(
    (nodeId: string) => {
      setSelection({ kind: "node", nodeId });
      // Reviewed the card — drop its "new" ring. Rows created with the node are
      // covered by the ring (never separately flagged), so they clear with it;
      // only rows added to an already-reviewed node carry their own tint.
      clearNewNode(nodeId);
    },
    [clearNewNode],
  );
  const selectResponsibility = useCallback(
    (nodeId: string, respId: string) => {
      setSelection({ kind: "responsibility", nodeId, respId });
      // Selecting a responsibility also selects its card — clear both.
      clearNewNode(nodeId);
      clearNewResp(respId);
    },
    [clearNewNode, clearNewResp],
  );
  const clearSelection = useCallback(() => setSelection(null), []);

  // Reveal an arbitrary node from anywhere (e.g. the drift panel). Walk its
  // parentId chain to rebuild the full path, drop to its parent's level so the
  // node shows as a card, and select it. Symbols/persons aren't standalone
  // cards, so reveal their owning parent instead.
  const revealNode = useCallback(
    (nodeId: string) => {
      const byId = new Map(modelRef.current.nodes.map((n) => [n.id, n]));
      let target = byId.get(nodeId);
      if (!target) return;
      if (target.kind === "symbol" || target.kind === "person") {
        const parent = target.parentId ? byId.get(target.parentId) : undefined;
        if (!parent) return;
        target = parent;
      }
      const ancestors: string[] = [];
      const seen = new Set<string>();
      let cur = target.parentId ?? null;
      while (cur && !seen.has(cur)) {
        seen.add(cur);
        ancestors.unshift(cur);
        cur = byId.get(cur)?.parentId ?? null;
      }
      setPath(ancestors);
      selectNode(target.id);
    },
    [selectNode],
  );

  // --- canvas layout intents ---

  const handlePlaceNode = useCallback(
    (nodeId: string, cell: Cell, newGroupId: string | null | undefined, measured: ReadonlyMap<string, Span>) => {
      updateModel((m) => placeNodeInModel(m, nodeId, cell, newGroupId, measured) ?? m);
    },
    [updateModel],
  );
  const handleMoveGroup = useCallback(
    (groupId: string, dRow: number, dCol: number, measured: ReadonlyMap<string, Span>) => {
      updateModel((m) => moveGroupInModel(m, groupId, dRow, dCol, measured) ?? m);
    },
    [updateModel],
  );
  const handleResizeGroup = useCallback(
    (groupId: string, size: { cols: number; rows: number }, measured: ReadonlyMap<string, Span>) => {
      updateModel((m) => resizeGroupInModel(m, groupId, size, measured) ?? m);
    },
    [updateModel],
  );
  // Seed positions for unplaced nodes/groups, re-flow overlapping placements,
  // and repair detached groups — all using the measured card spans. Runs after
  // PackBox measures the current surface.
  const handleAutoLayout = useCallback(
    (measured: ReadonlyMap<string, Span>) => {
      updateModel((m) => autoLayout(m, currentNodeId, measured));
    },
    [updateModel, currentNodeId],
  );

  // --- editor intents ---

  const editor = useMemo<Editor>(
    () => ({
      updateNode: (nodeId, patch) => {
        updateModel((m) => updateNodeHelper(m, nodeId, patch));
      },
      deleteNode: (nodeId) => {
        updateModel((m) => removeNodeHelper(m, nodeId));
      },
      addNode: (init) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addNodeHelper(m, {
            kind: init.kind,
            name: "",
            parentId: init.parentId,
            cell: init.cell,
            groupId: init.groupId,
          });
          newId = id;
          return next;
        });
        return newId;
      },
      updateGroup: (groupId, patch) => {
        updateModel((m) => updateGroupHelper(m, groupId, patch));
      },
      deleteGroup: (groupId) => {
        updateModel((m) => removeGroupHelper(m, groupId));
      },
      addGroup: (init) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addGroupHelper(m, {
            name: "",
            cell: init.cell,
            size: { cols: 4, rows: 4 },
            parentNodeId: init.parentNodeId,
          });
          newId = id;
          return next;
        });
        return newId;
      },
      addResponsibility: (host, hostId) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addResponsibility(m, host, hostId);
          newId = id;
          return next;
        });
        return newId;
      },
      updateResponsibility: (host, hostId, respId, patch) => {
        updateModel((m) => updateResponsibility(m, host, hostId, respId, patch));
      },
      removeResponsibility: (host, hostId, respId) => {
        updateModel((m) => {
          const resps = host === "node"
            ? m.nodes.find((n) => n.id === hostId)?.responsibilities
            : m.groups.find((g) => g.id === hostId)?.responsibilities;
          const deleted = resps?.find((r) => r.id === respId);
          let next = removeResponsibility(m, host, hostId, respId);
          if (deleted?.relocatedFrom) {
            next = unlockRelocatedSourceHelper(next, deleted);
          }
          return next;
        });
      },
      moveResponsibility: (fromNodeId, toNodeId, respId) => {
        updateModel((m) => moveResponsibilityHelper(m, fromNodeId, toNodeId, respId));
      },
      addProperty: (nodeId) => {
        updateModel((m) => addProperty(m, nodeId, "", ""));
      },
      updateProperty: (nodeId, index, patch) => {
        updateModel((m) => updateProperty(m, nodeId, index, patch));
      },
      removeProperty: (nodeId, index) => {
        updateModel((m) => removeProperty(m, nodeId, index));
      },
    }),
    [updateModel],
  );

  return (
    <ModelContext.Provider value={model}>
      <div className="flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
        <Breadcrumbs model={model} path={path} onJump={handleJump} projectPath={projectPath} />
        <div className="relative flex flex-1 min-h-0">
          <div className="relative min-w-0 flex-1">
          {/* One plain lock indicator. Live activity + Cancel live in the
              SyncBar (bottom powerline) — don't duplicate them here. */}
          {writing && (
            <div className="absolute top-0 inset-x-0 z-10 flex items-center justify-center pointer-events-none py-2">
              <span className="text-[11px] text-amber-400/70">
                {build.building
                  ? "Building the model"
                  : build.checking
                    ? "Checking for drift"
                    : `Filling ${agent.label}`}
                {build.active && build.phase ? ` — ${build.phase}` : " — canvas locked"}
              </span>
            </div>
          )}
          <PanZoom resetKey={currentNodeId ?? "__root__"}>
            <Surface
              view={surfaceView}
              parentNodeId={currentNodeId}
              ancestorAltitudes={ancestorAltitudes}
              editor={writing ? undefined : editor}
              selection={selection}
              onSelectNode={selectNode}
              onSelectResponsibility={selectResponsibility}
              onClearSelection={clearSelection}
              onNavigate={handleNavigate}
              onBack={handleJump}
              onPlaceNode={handlePlaceNode}
              onMoveGroup={handleMoveGroup}
              onResizeGroup={handleResizeGroup}
              onAutoLayout={handleAutoLayout}
              projectPath={projectPath}
              modelRef={modelRefStr}
              agent={agent}
              busy={writing}
              activeNodeIds={build.active ? build.activeNodeIds : EMPTY_IDS}
              newNodeIds={newNodeIds}
              newRespIds={newRespIds}
              generating={
                build.active
                  ? // Wave 1 (no active node yet) lights up whatever level is
                    // still forming; Wave 2 lights up only a container being filled.
                    build.activeNodeIds.size === 0 ||
                    (currentNodeId != null && build.activeNodeIds.has(currentNodeId))
                  : agent.running
              }
            />
          </PanZoom>
          </div>
          {selection && (
            <InspectorPanel
              model={model}
              selection={selection}
              projectPath={projectPath}
              onSelectNode={selectNode}
              onSelectResponsibility={selectResponsibility}
              onAdoptResponsibility={(nodeId, respId) =>
                updateModel((m) =>
                  updateResponsibility(m, "node", nodeId, respId, { vagrant: undefined }),
                )
              }
              onRejectResponsibility={(nodeId, respId) =>
                updateModel((m) => removeResponsibility(m, "node", nodeId, respId))
              }
              onClose={clearSelection}
            />
          )}
        </div>
        <SyncBar
          model={model}
          agent={agent}
          build={build}
          projectPath={projectPath}
          driftScopes={driftScopes}
          onRevealNode={revealNode}
          onOpenSettings={() => setSettingsOpen(true)}
        />
        {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      </div>
    </ModelContext.Provider>
  );
}
