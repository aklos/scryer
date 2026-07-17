/**
 * Throwaway browser harness for exercising DiagramView's drag behavior with
 * Playwright — NOT part of the product. Renders the real DiagramView against a
 * small in-memory model, applies onMoveNode through the real setNodePosition
 * helper, and exposes hooks on `window` for the test script:
 *
 *   window.__moves    — every onMoveNode call, in order
 *   window.__model    — the current model (positions included)
 *   window.__setFocus — drill to a level, like the breadcrumb would
 */

import React from "react";
import ReactDOM from "react-dom/client";
import "../src/index.css";
import { DiagramView } from "../src/DiagramView";
import { setNodePosition, type ScryModel } from "../src/viewmodel";
import type { ModelDiff } from "../src/planDiff";

const EMPTY_DIFF: ModelDiff = { changes: [] };

function mkModel(): ScryModel {
  const sym = (id: string, name: string) => ({
    id,
    kind: "symbol" as const,
    name,
    parentId: "comp-1",
    responsibilities: [],
  });
  return {
    version: "0.3",
    nodes: [
      { id: "sys-a", kind: "system", name: "Shop", responsibilities: [] },
      { id: "sys-b", kind: "system", name: "Billing", responsibilities: [] },
      { id: "con-1", kind: "container", name: "API", parentId: "sys-a", responsibilities: [] },
      { id: "con-2", kind: "container", name: "Web", parentId: "sys-a", responsibilities: [] },
      { id: "con-3", kind: "container", name: "Worker", parentId: "sys-a", responsibilities: [] },
      { id: "comp-1", kind: "component", name: "Checkout", parentId: "con-1", responsibilities: [] },
      ...[
        "parseCart",
        "priceItems",
        "applyDiscounts",
        "reserveStock",
        "createOrder",
        "chargeCard",
        "emitReceipt",
        "auditLog",
      ].map((n, i) => sym(`sym-${i}`, n)),
    ],
    links: [
      { id: "l1", src: "sys-a", dst: "sys-b", label: "bills via" },
      { id: "l2", src: "con-1", dst: "con-3", label: "enqueues" },
      { id: "l3", src: "con-2", dst: "con-1", label: "calls" },
      { id: "l4", src: "sym-0", dst: "sym-1", label: "" },
      { id: "l5", src: "sym-1", dst: "sym-2", label: "" },
      { id: "l6", src: "sym-2", dst: "sym-4", label: "" },
      { id: "l7", src: "sym-3", dst: "sym-4", label: "" },
      { id: "l8", src: "sym-4", dst: "sym-5", label: "" },
      { id: "l9", src: "sym-5", dst: "sym-6", label: "" },
      { id: "l10", src: "sym-4", dst: "sym-7", label: "" },
    ],
    groups: [],
  };
}

declare global {
  interface Window {
    __moves: Array<{ id: string; position: { x: number; y: number } | null }>;
    __model: ScryModel;
    __setFocus: (id: string | null) => void;
  }
}

window.__moves = [];

function Harness() {
  const [model, setModel] = React.useState<ScryModel>(mkModel);
  const [focusId, setFocusId] = React.useState<string | null>(null);
  const [selectedId, setSelectedId] = React.useState<string | null>(null);

  window.__model = model;
  window.__setFocus = setFocusId;

  const onMoveNode = React.useCallback(
    (id: string, position: { x: number; y: number } | null) => {
      window.__moves.push({ id, position });
      setModel((m) => setNodePosition(m, id, position));
    },
    [],
  );

  return (
    <div style={{ width: "100vw", height: "100vh", display: "flex" }}>
      <DiagramView
        model={model}
        planDiff={EMPTY_DIFF}
        committed={null}
        report={null}
        focusId={focusId}
        selectedId={selectedId}
        onFocus={setFocusId}
        onSelectNode={setSelectedId}
        onMoveNode={onMoveNode}
      />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Harness />
  </React.StrictMode>,
);
