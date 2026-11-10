/**
 * Connection handles on a diagram card — four sides plus four corners, all
 * source-type, matching the handle ids `assignAllHandles` chooses. Ported from
 * the pre-pivot canvas. In the read-only diagram they carry no editing
 * affordance: they're the anchor points edges attach to, invisible and not
 * connectable, so no part of a card shows a connect cursor.
 */

import { Handle, Position } from "@xyflow/react";

const base = { opacity: 0 } as const;
/** Handles default to connectable whatever the map says; these never are. */
const anchor = { type: "source", isConnectable: false } as const;

export function NodeHandles() {
  return (
    <>
      <Handle {...anchor} position={Position.Top} id="top" style={base} />
      <Handle {...anchor} position={Position.Bottom} id="bottom" style={base} />
      <Handle {...anchor} position={Position.Left} id="left" style={base} />
      <Handle {...anchor} position={Position.Right} id="right" style={base} />
      <Handle {...anchor} position={Position.Top} id="top-left" style={{ ...base, left: 0 }} />
      <Handle {...anchor} position={Position.Top} id="top-right" style={{ ...base, left: "100%" }} />
      <Handle {...anchor} position={Position.Bottom} id="bottom-left" style={{ ...base, left: 0 }} />
      <Handle {...anchor} position={Position.Bottom} id="bottom-right" style={{ ...base, left: "100%" }} />
    </>
  );
}

/** Single centered handle — for dot-tier nodes that connect center-to-center. */
export function CenterHandle() {
  return (
    <Handle
      {...anchor}
      position={Position.Top}
      id="c"
      style={{ opacity: 0, left: "50%", top: "50%" }}
    />
  );
}
