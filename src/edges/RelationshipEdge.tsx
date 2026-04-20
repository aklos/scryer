import { useContext } from "react";
import { BaseEdge, EdgeLabelRenderer, Position, useStore, type EdgeProps, type ReactFlowState } from "@xyflow/react";
import type { C4Edge, C4NodeData, Status } from "../types";
import { statusHex } from "../statusColors";
import { getThemedHex, ThemeContext } from "../theme";
import { StraightEdgesContext } from ".";

const CORNER_HANDLES = new Set(["top-left", "top-right", "bottom-left", "bottom-right"]);
const CURVE_OFFSET = 24;
const ENDPOINT_OFFSET = 5;
const BEZIER_CURVATURE = 0.25;

const STATUS_PRIORITY: Record<Status, number> = {
  proposed: 4,
  implemented: 3,
  verified: 1,
  vagrant: 2,
};

function bezierControlOffset(distance: number): number {
  if (distance >= 0) return 0.5 * distance;
  return BEZIER_CURVATURE * 25 * Math.sqrt(-distance);
}

function bezierControlPoint(pos: Position, x: number, y: number, otherX: number, otherY: number): [number, number] {
  switch (pos) {
    case Position.Left:   return [x - bezierControlOffset(x - otherX), y];
    case Position.Right:  return [x + bezierControlOffset(otherX - x), y];
    case Position.Top:    return [x, y - bezierControlOffset(y - otherY)];
    case Position.Bottom: return [x, y + bezierControlOffset(otherY - y)];
  }
}

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
  useContext(ThemeContext);
  const straightEdges = useContext(StraightEdgesContext);

  const isBiDirectional = useStore((s: ReactFlowState) =>
    s.edges.some((e) => e.source === target && e.target === source),
  );

  const inferredStatus = useStore((s: ReactFlowState) => {
    const resolveStatus = (nodeId: string): Status | undefined => {
      const node = s.nodeLookup.get(nodeId);
      const nd = node?.data as C4NodeData | undefined;
      if (!nd) return undefined;
      if (nd.kind === "person" || (nd.kind === "system" && nd.external)) return undefined;
      if (nd.status) return nd.status;
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
  const baseColor = inferredStatus ? statusHex(inferredStatus) : getThemedHex("slate", "400");
  const selColor = getComputedStyle(document.documentElement).getPropertyValue("--selection-color").trim() || "#18181b";
  const mentionColor = getThemedHex("zinc", "400");
  const edgeColor = selected ? selColor : isMention ? mentionColor : baseColor;

  let edgePath: string;
  let labelX: number;
  let labelY: number;
  let arrowEndX: number;
  let arrowEndY: number;
  let arrowAngle: number;

  const route = data?._route as { x: number; y: number }[] | undefined;
  if (route && route.length >= 1) {
    // Orthogonal polyline with rounded corners
    const pts = [{ x: sourceX, y: sourceY }, ...route, { x: targetX, y: targetY }];
    const R = 30;
    let d = `M ${pts[0].x} ${pts[0].y}`;
    for (let i = 1; i < pts.length - 1; i++) {
      const prev = pts[i - 1], cur = pts[i], next = pts[i + 1];
      const toPrevX = prev.x - cur.x, toPrevY = prev.y - cur.y;
      const toNextX = next.x - cur.x, toNextY = next.y - cur.y;
      const lenPrev = Math.sqrt(toPrevX * toPrevX + toPrevY * toPrevY) || 1;
      const lenNext = Math.sqrt(toNextX * toNextX + toNextY * toNextY) || 1;
      const r = Math.min(R, lenPrev / 2, lenNext / 2);
      const startX = cur.x + (toPrevX / lenPrev) * r;
      const startY = cur.y + (toPrevY / lenPrev) * r;
      const endX = cur.x + (toNextX / lenNext) * r;
      const endY = cur.y + (toNextY / lenNext) * r;
      d += ` L ${startX} ${startY} Q ${cur.x} ${cur.y} ${endX} ${endY}`;
    }
    d += ` L ${pts[pts.length - 1].x} ${pts[pts.length - 1].y}`;
    edgePath = d;

    const segs = [{ x: sourceX, y: sourceY }, ...route, { x: targetX, y: targetY }];
    let bestLen = 0, bestMidX = sourceX, bestMidY = sourceY;
    for (let i = 0; i < segs.length - 1; i++) {
      const segLen = Math.sqrt((segs[i+1].x - segs[i].x) ** 2 + (segs[i+1].y - segs[i].y) ** 2);
      if (segLen > bestLen) {
        bestLen = segLen;
        bestMidX = (pts[i].x + pts[i+1].x) / 2;
        bestMidY = (pts[i].y + pts[i+1].y) / 2;
      }
    }
    labelX = bestMidX;
    labelY = bestMidY;

    const lastPt = pts[pts.length - 2];
    arrowEndX = targetX;
    arrowEndY = targetY;
    arrowAngle = Math.atan2(targetY - lastPt.y, targetX - lastPt.x);

  } else if (straightEdges && !isMention) {
    // Straight line
    edgePath = `M ${sourceX} ${sourceY} L ${targetX} ${targetY}`;
    labelX = (sourceX + targetX) / 2;
    labelY = (sourceY + targetY) / 2;
    arrowEndX = targetX;
    arrowEndY = targetY;
    arrowAngle = Math.atan2(targetY - sourceY, targetX - sourceX);

  } else if (isMention) {
    edgePath = `M ${sourceX} ${sourceY} L ${targetX} ${targetY}`;
    labelX = (sourceX + targetX) / 2;
    labelY = (sourceY + targetY) / 2;
    arrowEndX = targetX;
    arrowEndY = targetY;
    arrowAngle = Math.atan2(targetY - sourceY, targetX - sourceX);

  } else if (isBiDirectional) {
    // Cubic bezier with perpendicular offset so parallel edges don't cross
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

    const t = 0.25;
    const u = 1 - t;
    labelX = u*u*u*sx + 3*u*u*t*cp1x + 3*u*t*t*cp2x + t*t*t*tx;
    labelY = u*u*u*sy + 3*u*u*t*cp1y + 3*u*t*t*cp2y + t*t*t*ty;

    arrowEndX = tx;
    arrowEndY = ty;
    arrowAngle = Math.atan2(ty - cp2y, tx - cp2x);

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

    arrowEndX = targetX;
    arrowEndY = targetY;
    arrowAngle = Math.atan2(targetY - cp2y, targetX - cp2x);

  } else {
    // Standard single bezier — compute control points explicitly
    const [cp1x, cp1y] = bezierControlPoint(sourcePosition, sourceX, sourceY, targetX, targetY);
    const [cp2x, cp2y] = bezierControlPoint(targetPosition, targetX, targetY, sourceX, sourceY);
    edgePath = `M ${sourceX},${sourceY} C ${cp1x},${cp1y} ${cp2x},${cp2y} ${targetX},${targetY}`;
    labelX = 0.125 * sourceX + 0.375 * cp1x + 0.375 * cp2x + 0.125 * targetX;
    labelY = 0.125 * sourceY + 0.375 * cp1y + 0.375 * cp2y + 0.125 * targetY;

    arrowEndX = targetX;
    arrowEndY = targetY;
    arrowAngle = Math.atan2(targetY - cp2y, targetX - cp2x);
  }

  // Arrowhead polygon from explicit geometry
  const arrowSize = 8;
  const ax1 = arrowEndX - arrowSize * Math.cos(arrowAngle - Math.PI / 6);
  const ay1 = arrowEndY - arrowSize * Math.sin(arrowAngle - Math.PI / 6);
  const ax2 = arrowEndX - arrowSize * Math.cos(arrowAngle + Math.PI / 6);
  const ay2 = arrowEndY - arrowSize * Math.sin(arrowAngle + Math.PI / 6);

  const clipId = `clip-${id}`;
  const edgeOpacity = isMention
    ? (connectedHighlight ? 0.6 : dimmed ? 0.15 : 0.4)
    : selected || connectedHighlight ? 1 : dimmed ? 0.25 : 0.7;

  return (
    <>
      <defs>
        <clipPath id={clipId}>
          {(() => {
            const perpXDir = -Math.sin(arrowAngle);
            const perpYDir = Math.cos(arrowAngle);
            const bx = (ax1 + ax2) / 2;
            const by = (ay1 + ay2) / 2;
            const far = 10000;
            return (
              <path d={`M ${bx + perpXDir * far} ${by + perpYDir * far} L ${bx - perpXDir * far} ${by - perpYDir * far} L ${bx - perpXDir * far - Math.cos(arrowAngle) * far} ${by - perpYDir * far - Math.sin(arrowAngle) * far} L ${bx + perpXDir * far - Math.cos(arrowAngle) * far} ${by + perpYDir * far - Math.sin(arrowAngle) * far} Z`} />
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
        {!isMention && (
          <polygon
            points={`${arrowEndX},${arrowEndY} ${ax1},${ay1} ${ax2},${ay2}`}
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
              <div className="px-1.5 py-0.5 text-[10px] rounded bg-[var(--surface-active)]/80 text-[var(--text)] whitespace-nowrap">
                {label}
              </div>
            )}
            {method && (
              <div className="text-[9px] text-[var(--text-tertiary)] whitespace-nowrap">
                [{method}]
              </div>
            )}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
