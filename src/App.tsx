/**
 * App shell.
 *
 * Holds the v0.3 model in storage (project-local on disk, auto-saved) and the
 * navigation path. The current SurfaceView is derived per render from
 * (model, currentNodeId). Mutations come back from Surface as Editor intents.
 */

import { useCallback, useMemo, useRef, useState } from "react";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import { Breadcrumbs } from "./Breadcrumbs";
import { PanZoom } from "./PanZoom";
import { Surface } from "./Surface";
import { ModelContext } from "./modelcontext";
import { ProjectPicker } from "./ProjectPicker";
import { SyncBar } from "./SyncBar";
import { useModelStorage } from "./hooks/useModelStorage";
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
  type ScryModel,
} from "./viewmodel";
import {
  moveGroupInModel,
  placeNodeInModel,
  resizeGroupInModel,
  resolveOverlaps,
  type Span,
} from "./pack";
import type { Editor } from "./editor";

export default function App() {
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
  const model = storage.model;

  if (!model || storage.status !== "ready") {
    return <ProjectPicker storage={storage} />;
  }
  return (
    <Canvas
      model={model}
      updateModel={storage.updateModel}
      projectPath={storage.projectPath}
      modelRef={storage.modelRef}
    />
  );
}

function Canvas({
  model,
  updateModel,
  projectPath,
  modelRef: modelRefStr,
}: {
  model: ScryModel;
  updateModel: ReturnType<typeof useModelStorage>["updateModel"];
  projectPath: string | null;
  modelRef: string | null;
}) {
  const agent = useAgentSession();
  const [path, setPath] = useState<string[]>([]);
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
    if (node.kind === "operation" || node.kind === "model" || node.kind === "person") return;
    setPath((p) => (p[p.length - 1] === nodeId ? p : [...p, nodeId]));
  }, []);

  const handleJump = useCallback((index: number) => {
    setPath((p) => (index < 0 ? [] : p.slice(0, index + 1)));
  }, []);

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
  const handleFixOverlaps = useCallback(
    (measuredSpans: ReadonlyMap<string, { w: number; h: number }>) => {
      updateModel((m) => resolveOverlaps(m, measuredSpans));
    },
    [updateModel],
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
        <Breadcrumbs model={model} path={path} onJump={handleJump} />
        <div className="relative flex-1">
          {agent.running && (
            <div className="absolute top-0 inset-x-0 z-10 flex items-center justify-center pointer-events-none py-2">
              <span className="text-[11px] text-amber-400/70">
                Canvas locked while agent is filling {agent.label}
              </span>
            </div>
          )}
          <PanZoom resetKey={currentNodeId ?? "__root__"}>
            <Surface
              view={surfaceView}
              parentNodeId={currentNodeId}
              ancestorAltitudes={ancestorAltitudes}
              editor={agent.running ? undefined : editor}
              onNavigate={handleNavigate}
              onBack={handleJump}
              onPlaceNode={handlePlaceNode}
              onMoveGroup={handleMoveGroup}
              onResizeGroup={handleResizeGroup}
              onFixOverlaps={handleFixOverlaps}
              projectPath={projectPath}
              modelRef={modelRefStr}
              agent={agent}
            />
          </PanZoom>
        </div>
        <SyncBar model={model} agent={agent} projectPath={projectPath} />
      </div>
    </ModelContext.Provider>
  );
}
