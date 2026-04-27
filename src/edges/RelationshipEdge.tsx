import { useContext } from "react";
import { BaseEdge, EdgeLabelRenderer, useStore, type EdgeProps, type ReactFlowState } from "@xyflow/react";
import type { C4Edge, C4NodeData, Status } from "../types";
import { statusHex } from "../statusColors";
import { getThemedHex, ThemeContext } from "../theme";

const STATUS_PRIORITY: Record<Status, number> = {
  proposed: 4,
  implemented: 3,
  verified: 1,
  vagrant: 2,
};

export function RelationshipEdge({
  id,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  selected,
}: EdgeProps<C4Edge>) {
  useContext(ThemeContext);

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

  const PAIR_SHIFT = 4;
  const biPair = data?._biPair;
  let ox = 0, oy = 0;
  if (biPair) {
    const dx = targetX - sourceX;
    const dy = targetY - sourceY;
    const len = Math.sqrt(dx * dx + dy * dy) || 1;
    ox = (-dy / len) * PAIR_SHIFT;
    oy = (dx / len) * PAIR_SHIFT;
  }

  const sx = sourceX + ox, sy = sourceY + oy;
  const tx = targetX + ox, ty = targetY + oy;

  let edgePath: string;
  let labelX: number;
  let labelY: number;
  let arrowEndX: number;
  let arrowEndY: number;
  let arrowAngle: number;

  const route = data?._route as { x: number; y: number }[] | undefined;
  if (route && route.length >= 1) {
    const pts = [{ x: sx, y: sy }, ...route.map(p => ({ x: p.x + ox, y: p.y + oy })), { x: tx, y: ty }];
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

    let bestLen = 0, bestMidX = sx, bestMidY = sy;
    for (let i = 0; i < pts.length - 1; i++) {
      const segLen = Math.sqrt((pts[i+1].x - pts[i].x) ** 2 + (pts[i+1].y - pts[i].y) ** 2);
      if (segLen > bestLen) {
        bestLen = segLen;
        bestMidX = (pts[i].x + pts[i+1].x) / 2;
        bestMidY = (pts[i].y + pts[i+1].y) / 2;
      }
    }
    labelX = bestMidX;
    labelY = bestMidY;

    const lastPt = pts[pts.length - 2];
    arrowEndX = tx;
    arrowEndY = ty;
    arrowAngle = Math.atan2(ty - lastPt.y, tx - lastPt.x);

  } else {
    edgePath = `M ${sx} ${sy} L ${tx} ${ty}`;
    labelX = (sx + tx) / 2;
    labelY = (sy + ty) / 2;
    arrowEndX = tx;
    arrowEndY = ty;
    arrowAngle = Math.atan2(ty - sy, tx - sx);
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
              transform: `translate(-50%, -50%) translate(${biPair ? labelX + ox * 2.5 : labelX}px,${biPair ? labelY + oy * 2.5 : labelY}px)`,
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
