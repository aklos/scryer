/**
 * Interactive smoke test for the lifted shell (route `#shell`).
 *
 * Not a filmed scene — just the real `WorkspaceShell` wired to local `useState`
 * so the navigation actually works (click the tree, flip Wiki/Map, drill the
 * diagram). It exists to prove the real app renders and navigates on the
 * Aperture Pay fixtures before any camera choreography goes on top.
 */

import { useCallback, useState } from "react";
import type { WorkspaceView } from "../../src/TopBar";
import type { Selected } from "../../src/NodePage";
import { paymentsModel, committedModel, healthReport } from "../fixtures";
import { WorkspaceShell, IDLE_AGENT, IDLE_BUILD, type WorkspaceState } from "./Workspace";

const EMPTY: ReadonlySet<string> = new Set();

/** Parent chain of `id`, so selecting a node reveals it in the tree. */
function ancestors(id: string): string[] {
  const out: string[] = [];
  let cur = paymentsModel.nodes.find((n) => n.id === id);
  while (cur?.parentId) {
    out.push(cur.parentId);
    cur = paymentsModel.nodes.find((n) => n.id === cur!.parentId);
  }
  return out;
}

const INITIAL: WorkspaceState = {
  model: paymentsModel,
  committed: committedModel,
  projectPath: "/demo/aperture-pay",
  view: "wiki",
  selected: { kind: "node", id: "ledger" },
  expanded: new Set(["aperture"]),
  diagramFocus: "aperture",
  driftScopes: [],
  newNodeIds: EMPTY,
  newRespIds: EMPTY,
  health: healthReport,
  agent: IDLE_AGENT,
  build: IDLE_BUILD,
};

export function ShellDemo() {
  const [state, setState] = useState<WorkspaceState>(INITIAL);

  const onSelectNode = useCallback((id: string) => {
    setState((s) => {
      const node = s.model.nodes.find((n) => n.id === id);
      return {
        ...s,
        selected: { kind: "node", id } as Selected,
        expanded: new Set([...s.expanded, ...ancestors(id)]),
        diagramFocus: node?.parentId ?? null,
      };
    });
  }, []);

  // Selecting a node ON the diagram highlights it without reframing the level —
  // otherwise every card click re-focuses to its parent and rebuilds the scene.
  const onSelectFromDiagram = useCallback((id: string) => {
    setState((s) => ({
      ...s,
      selected: { kind: "node", id } as Selected,
      expanded: new Set([...s.expanded, ...ancestors(id)]),
    }));
  }, []);

  const onToggle = useCallback((id: string, expand?: boolean) => {
    setState((s) => {
      const has = s.expanded.has(id);
      const next = new Set(s.expanded);
      if (expand === true || (expand === undefined && !has)) next.add(id);
      else next.delete(id);
      return { ...s, expanded: next };
    });
  }, []);

  const onSetView = useCallback((view: WorkspaceView) => {
    setState((s) => ({ ...s, view }));
  }, []);

  const onFocus = useCallback((id: string | null) => {
    setState((s) => ({
      ...s,
      diagramFocus: id,
      expanded: id ? new Set([...s.expanded, id, ...ancestors(id)]) : s.expanded,
    }));
  }, []);

  return (
    // The shell now sizes to its parent (h-full/w-full) so the film can tile it
    // beside the terminal; this wrapper keeps the standalone smoke test full-screen.
    <div className="h-screen w-screen">
      <WorkspaceShell
        state={state}
        actions={{ onSelectNode, onSelectFromDiagram, onToggle, onSetView, onFocus }}
      />
    </div>
  );
}
