/**
 * The exported viewer's root. A read-only, navigational projection of a single
 * model — the same `DiagramView` the desktop app renders, lifted into a static
 * page. There is no tree, no wiki page, and no editing: the diagram owns the
 * whole window and drives its own focus (drill-down) and selection state, so a
 * recipient can pan, zoom, and double-click into containers exactly as in the
 * app, with nothing behind it but the baked-in model.
 */

import { useState } from "react";
import type { ScryModel } from "../src/viewmodel";
import { EMPTY_DIFF } from "../src/planDiff";
import { DiagramView } from "../src/DiagramView";

export function ExportApp({ model }: { model: ScryModel }) {
  // The level being shown (children of this node; null = top level) and the
  // highlighted node. DiagramView is otherwise self-contained — it renders its
  // own breadcrumb and handles drill-down through these two callbacks.
  const [focusId, setFocusId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <div className="h-full w-full">
      <DiagramView
        model={model}
        planDiff={EMPTY_DIFF}
        report={null}
        focusId={focusId}
        selectedId={selectedId}
        onFocus={setFocusId}
        onSelectNode={setSelectedId}
      />
    </div>
  );
}
