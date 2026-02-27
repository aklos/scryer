import { BaseEdge, EdgeLabelRenderer, getBezierPath, Position, useStore, type EdgeProps, type ReactFlowState } from "@xyflow/react";
import type { C4Edge, C4NodeData, Status } from "../types";
import { STATUS_COLORS } from "../statusColors";

const CORNER_HANDLES = new Set(["top-left", "top-right", "bottom-left", "bottom-right"]);
const STROKE_COLOR = "#94a3b8"; // slate-400
const CURVE_OFFSET = 24;
const ENDPOINT_OFFSET = 5;

const STATUS_PRIORITY: Record<Status, number> = {
  deprecated: 4,
  proposed: 3,
  changed: 2,
  implemented: 1,
};

export function RelationshipEdge({
  id,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  sourceHandleId,
  targetHandleId,
  data,
  selected,
}: EdgeProps<C4Edge>) {
  // Check if a reverse edge exists between the same pair of nodes
  const isBiDirectional = useStore((s: ReactFlowState) =>
    s.edges.some((e) => e.source === target && e.target === source),
  );

  // Infer edge status from connected node statuses (ignore the edge's own data.status).
  // Skip person and external-system nodes — they aren't implementable artifacts.
  // For group boxes, derive status from the worst status among member nodes.
  const inferredStatus = useStore((s: ReactFlowState) => {
    const resolveStatus = (nodeId: string): Status | undefined => {
      const node = s.nodeLookup.get(nodeId);
      const nd = node?.data as C4NodeData | undefined;
      if (!nd) return undefined;
      if (nd.kind === "person" || (nd.kind === "system" && nd.external)) return undefined;
      if (nd.status) return nd.status;
      // Group box nodes carry _memberIds — derive worst status from members
      const memberIds = (nd as Record<string, unknown>)._memberIds as string[] | undefined;
      if (node?.type === "groupBox" && memberIds) {
        let worst: Status | undefined;
        for (const mid of memberIds) {
          const member = s.nodeLookup.get(mid);
          const ms = (member?.data as C4NodeData | undefined)?.status;
          if (ms && (!worst || STATUS_PRIORITY[ms] > STATUS_PRIORITY[worst])) {
            worst = ms;
          }
        }
        return worst;
      }
      return undefined;
    };
    const srcStatus = resolveStatus(source);
    const tgtStatus = resolveStatus(target);
    if (srcStatus && tgtStatus) {
      return STATUS_PRIORITY[srcStatus] >= STATUS_PRIORITY[tgtStatus] ? srcStatus : tgtStatus;
    }
    return srcStatus ?? tgtStatus ?? undefined;
  });

  const connectedHighlight = data?._highlighted;
  const dimmed = data?._dimmed;
  const isMention = data?._mention;
  const label = data?.label;
  const method = data?.method;
  const statusColor = inferredStatus ? STATUS_COLORS[inferredStatus] : null;
  const baseColor = statusColor?.hex ?? STROKE_COLOR;
  const selColor = getComputedStyle(document.documentElement).getPropertyValue("--selection-color").trim() || "#18181b";
  const edgeColor = selected ? selColor : isMention ? "#a1a1aa" : baseColor;

  let edgePath: string;
  let labelX: number;
  let labelY: number;

  if (isMention) {
    edgePath = `M ${sourceX} ${sourceY} L ${targetX} ${targetY}`;
    labelX = (sourceX + targetX) / 2;
    labelY = (sourceY + targetY) / 2;
  } else if (isBiDirectional) {
    // Cubic bezier with perpendicular offset so parallel edges never cross.
    const canonicalDx = source < target ? targetX - sourceX : sourceX - targetX;
    const canonicalDy = source < target ? targetY - sourceY : sourceY - targetY;
    const len = Math.sqrt(canonicalDx * canonicalDx + canonicalDy * canonicalDy) || 1;
    const perpX = -canonicalDy / len;
    const perpY = canonicalDx / len;
    const sign = source < target ? 1 : -1;

    const dx = targetX - sourceX;
    const dy = targetY - sourceY;

    const sx = sourceX + perpX * ENDPOINT_OFFSET * sign;
    const sy = sourceY + perpY * ENDPOINT_OFFSET * sign;
    const tx = targetX + perpX * ENDPOINT_OFFSET * sign;
    const ty = targetY + perpY * ENDPOINT_OFFSET * sign;

    const tangentLen = len * 0.3;
    const cp1x = sx + (dx / len) * tangentLen + perpX * CURVE_OFFSET * sign;
    const cp1y = sy + (dy / len) * tangentLen + perpY * CURVE_OFFSET * sign;
    const cp2x = tx - (dx / len) * tangentLen + perpX * CURVE_OFFSET * sign;
    const cp2y = ty - (dy / len) * tangentLen + perpY * CURVE_OFFSET * sign;

    edgePath = `M ${sx} ${sy} C ${cp1x} ${cp1y} ${cp2x} ${cp2y} ${tx} ${ty}`;

    labelX = 0.125 * sx + 0.375 * cp1x + 0.375 * cp2x + 0.125 * tx;
    labelY = 0.125 * sy + 0.375 * cp1y + 0.375 * cp2y + 0.125 * ty;
  } else if (CORNER_HANDLES.has(sourceHandleId ?? "") || CORNER_HANDLES.has(targetHandleId ?? "")) {
    // Corner handles — cubic bezier where each end leaves at 45 degrees
    const dist = Math.sqrt((targetX - sourceX) ** 2 + (targetY - sourceY) ** 2) || 1;
    const cpLen = dist * 0.4;

    const cornerDir = (handleId: string | null | undefined): [number, number] => {
      switch (handleId) {
        case "top-left":     return [-1, -1];
        case "top-right":    return [ 1, -1];
        case "bottom-left":  return [-1,  1];
        case "bottom-right": return [ 1,  1];
        default:             return [0, 0];
      }
    };
    const INV_SQRT2 = 1 / Math.sqrt(2);

    const positionOffset = (p: Position): [number, number] => {
      switch (p) {
        case Position.Top:    return [0, -1];
        case Position.Bottom: return [0,  1];
        case Position.Left:   return [-1, 0];
        case Position.Right:  return [ 1, 0];
      }
    };

    let cp1x: number, cp1y: number;
    const [sdx, sdy] = cornerDir(sourceHandleId);
    if (sdx !== 0) {
      cp1x = sourceX + sdx * INV_SQRT2 * cpLen;
      cp1y = sourceY + sdy * INV_SQRT2 * cpLen;
    } else {
      const [ox, oy] = positionOffset(sourcePosition);
      cp1x = sourceX + ox * cpLen;
      cp1y = sourceY + oy * cpLen;
    }

    let cp2x: number, cp2y: number;
    const [tdx, tdy] = cornerDir(targetHandleId);
    if (tdx !== 0) {
      cp2x = targetX + tdx * INV_SQRT2 * cpLen;
      cp2y = targetY + tdy * INV_SQRT2 * cpLen;
    } else {
      const [ox, oy] = positionOffset(targetPosition);
      cp2x = targetX + ox * cpLen;
      cp2y = targetY + oy * cpLen;
    }

    edgePath = `M ${sourceX} ${sourceY} C ${cp1x} ${cp1y} ${cp2x} ${cp2y} ${targetX} ${targetY}`;
    labelX = 0.125 * sourceX + 0.375 * cp1x + 0.375 * cp2x + 0.125 * targetX;
    labelY = 0.125 * sourceY + 0.375 * cp1y + 0.375 * cp2y + 0.125 * targetY;
  } else {
    // Single edge — bezier through cardinal handle positions
    [edgePath, labelX, labelY] = getBezierPath({
      sourceX, sourceY, sourcePosition,
      targetX, targetY, targetPosition,
    });
  }

  // Compute arrowhead at the end of the path using the SVG path's actual endpoint and tangent.
  // Parse the last two points from the path to get the true tangent direction.
  const arrowSize = 8;
  let endX: number, endY: number, angle: number;

  {
    // Extract all numbers from the SVG path
    const nums = edgePath.match(/-?[\d.]+/g)?.map(Number) ?? [];
    // Last two numbers are the endpoint
    endX = nums[nums.length - 2] ?? targetX;
    endY = nums[nums.length - 1] ?? targetY;
    // For cubic bezier (C command), the second control point is 4 numbers before the end
    // For line (L command) or simple path, use the previous point
    let prevX: number, prevY: number;
    if (nums.length >= 8) {
      // Cubic bezier: second control point is at [-4, -3]
      prevX = nums[nums.length - 4];
      prevY = nums[nums.length - 3];
    } else {
      prevX = nums[0] ?? sourceX;
      prevY = nums[1] ?? sourceY;
    }
    angle = Math.atan2(endY - prevY, endX - prevX);
  }

  // Arrowhead base corners
  const ax1 = endX - arrowSize * Math.cos(angle - Math.PI / 6);
  const ay1 = endY - arrowSize * Math.sin(angle - Math.PI / 6);
  const ax2 = endX - arrowSize * Math.cos(angle + Math.PI / 6);
  const ay2 = endY - arrowSize * Math.sin(angle + Math.PI / 6);

  const clipId = `clip-${id}`;
  const edgeOpacity = isMention
    ? (connectedHighlight ? 0.6 : dimmed ? 0.08 : 0.3)
    : selected || connectedHighlight ? 1 : dimmed ? 0.15 : 0.55;

  return (
    <>
      <defs>
        {/* Clip: cut off everything past the arrowhead base line so the dashed line stops cleanly */}
        <clipPath id={clipId}>
          {(() => {
            // Half-plane: everything on the source side of the arrowhead base
            const perpXDir = -Math.sin(angle);
            const perpYDir = Math.cos(angle);
            const bx = (ax1 + ax2) / 2;
            const by = (ay1 + ay2) / 2;
            const far = 10000;
            // Four corners of a huge rect on the source side of the base line
            return (
              <path d={`M ${bx + perpXDir * far} ${by + perpYDir * far} L ${bx - perpXDir * far} ${by - perpYDir * far} L ${bx - perpXDir * far - Math.cos(angle) * far} ${by - perpYDir * far - Math.sin(angle) * far} L ${bx + perpXDir * far - Math.cos(angle) * far} ${by + perpYDir * far - Math.sin(angle) * far} Z`} />
            );
          })()}
        </clipPath>
      </defs>
      {/* Wider invisible hit area for easier clicking */}
      <path
        d={edgePath}
        fill="none"
        stroke="transparent"
        strokeWidth={20}
        className="react-flow__edge-interaction"
      />
      <g opacity={edgeOpacity}>
        <g clipPath={isMention ? undefined : `url(#${clipId})`}>
          <BaseEdge
            id={id}
            path={edgePath}
            style={{
              stroke: edgeColor,
              strokeWidth: isMention ? 1 : selected ? 2.5 : 1.5,
              strokeDasharray: isMention ? undefined : "6 3",
              animation: isMention ? undefined : "dash-flow 0.5s linear infinite",
            }}
          />
        </g>
        {/* Solid arrowhead — skip for mention edges */}
        {!isMention && (
          <polygon
            points={`${endX},${endY} ${ax1},${ay1} ${ax2},${ay2}`}
            fill={edgeColor}
          />
        )}
      </g>
      {/* Midpoint handle dot — visible on edge hover */}
      <circle
        cx={labelX}
        cy={labelY}
        r={4}
        fill={edgeColor}
        className="edge-handle-dot"
      />
      {(label || method) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              zIndex: 1,
              pointerEvents: "all",
              ...(dimmed ? { opacity: 0.25 } : {}),
            }}
            className="flex flex-col items-center"
          >
            {label && (
              <div className="px-1.5 py-0.5 text-[10px] rounded bg-zinc-200/80 text-zinc-700 dark:bg-black/60 dark:text-zinc-200 whitespace-nowrap">
                {label}
              </div>
            )}
            {method && (
              <div className="text-[9px] text-zinc-500 dark:text-zinc-400 whitespace-nowrap">
                [{method}]
              </div>
            )}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
