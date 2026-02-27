import { Handle, Position } from "@xyflow/react";

const hiddenStyle = { opacity: 0, pointerEvents: "none" as const };

export function NodeHandles({ hidden }: { hidden?: boolean } = {}) {
  const s = hidden ? hiddenStyle : undefined;
  return (
    <>
      <Handle type="source" position={Position.Top} id="top" isConnectable={!hidden} style={s} />
      <Handle type="source" position={Position.Bottom} id="bottom" isConnectable={!hidden} style={s} />
      <Handle type="source" position={Position.Left} id="left" isConnectable={!hidden} style={s} />
      <Handle type="source" position={Position.Right} id="right" isConnectable={!hidden} style={s} />
      <Handle type="source" position={Position.Top} id="top-left" isConnectable={!hidden} style={hidden ? { ...hiddenStyle, left: 0 } : { left: 0 }} />
      <Handle type="source" position={Position.Top} id="top-right" isConnectable={!hidden} style={hidden ? { ...hiddenStyle, left: "100%" } : { left: "100%" }} />
      <Handle type="source" position={Position.Bottom} id="bottom-left" isConnectable={!hidden} style={hidden ? { ...hiddenStyle, left: 0 } : { left: 0 }} />
      <Handle type="source" position={Position.Bottom} id="bottom-right" isConnectable={!hidden} style={hidden ? { ...hiddenStyle, left: "100%" } : { left: "100%" }} />
    </>
  );
}

/** Single centered handle — for nodes that only need mention edge anchoring. */
export function CenterHandle() {
  return (
    <Handle
      type="source"
      position={Position.Top}
      id="top"
      isConnectable={false}
      style={{ opacity: 0, pointerEvents: "none", left: "50%", top: "50%" }}
    />
  );
}
