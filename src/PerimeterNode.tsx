/**
 * Perimeter nodes — actors and reference cards that live in the ring bands
 * around the inner grid. All dimensions scale by `useZoom()` (figma-style).
 */

import { useId } from "react";
import type { ReactNode } from "react";
import type { Node } from "./viewmodel";
import { personPath, PERSON_VIEWBOX } from "./shapes";
import { useZoom } from "./PanZoom";
import { effectiveNodeStatus } from "./rollup";
import { STATUS_COLORS } from "./statusColors";
import { kindIcon } from "./kindIcon";

export const PERSON_W = 190;
export const PERSON_SVG_H = (PERSON_VIEWBOX.h / PERSON_VIEWBOX.w) * PERSON_W;

function PersonShape({
  children,
  nodeId,
  selected,
}: {
  children: ReactNode;
  nodeId?: string;
  selected?: boolean;
}) {
  const zoom = useZoom();
  const uid = useId();
  const fillId = `person-fill-${uid}`;
  const strokeId = `person-stroke-${uid}`;
  const w = PERSON_W * zoom;
  const h = PERSON_SVG_H * zoom;
  return (
    <div
      data-conn-node={nodeId}
      data-select-node={nodeId}
      data-pickup={nodeId}
      className="flex flex-col items-center"
      style={{ width: w, minHeight: h }}
    >
      <svg
        width={w}
        height={h}
        viewBox={`0 0 ${PERSON_VIEWBOX.w} ${PERSON_VIEWBOX.h}`}
        className="pointer-events-none"
        style={{ display: "block", marginBottom: -h * 0.45 }}
      >
        <defs>
          <linearGradient
            id={fillId}
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="64"
            x2="0"
            y2={PERSON_VIEWBOX.h}
          >
            <stop offset="0%" stopColor="var(--surface-raised)" stopOpacity="1" />
            <stop offset="100%" stopColor="var(--surface-raised)" stopOpacity="0" />
          </linearGradient>
          <linearGradient
            id={strokeId}
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="0"
            x2="0"
            y2={PERSON_VIEWBOX.h}
          >
            <stop offset="0%" stopColor="var(--border)" stopOpacity="0.8" />
            <stop offset="70%" stopColor="var(--border)" stopOpacity="0.3" />
            <stop offset="100%" stopColor="var(--border)" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path d={personPath(true)} fill={`url(#${fillId})`} />
        <path
          d={personPath(false)}
          fill="none"
          stroke={selected ? "var(--accent-blue)" : `url(#${strokeId})`}
          strokeWidth={selected ? 2.5 : 1.5}
        />
      </svg>
      <div
        className="flex w-full flex-col items-center text-center"
        style={{ padding: `0 ${20 * zoom}px` }}
      >
        {children}
      </div>
    </div>
  );
}

export type PerimeterVariant = "person" | "external" | "reference";

export function PerimeterNode({
  node,
  variant,
  selected = false,
}: {
  node: Node;
  variant: PerimeterVariant;
  selected?: boolean;
}) {
  const zoom = useZoom();

  if (variant === "person") {
    return (
      <PersonShape nodeId={node.id} selected={selected}>
        <div
          className="truncate font-semibold text-[var(--text-secondary)]"
          style={{ fontSize: 13 * zoom }}
        >
          {node.name}
        </div>
        {node.description && (
          <div
            className="leading-snug text-[var(--text-muted)]"
            style={{ marginTop: 2 * zoom, fontSize: 11 * zoom }}
          >
            {node.description}
          </div>
        )}
      </PersonShape>
    );
  }

  const Icon = kindIcon(node);
  const eff = effectiveNodeStatus(node);
  const iconColor =
    variant === "external" || !eff
      ? "text-[var(--text-muted)]"
      : STATUS_COLORS[eff].icon;

  return (
    <div
      data-conn-node={node.id}
      data-select-node={node.id}
      data-pickup={node.id}
      className="border bg-[var(--surface-canvas)]"
      style={{
        width: PERSON_W * zoom,
        padding: `${12 * zoom}px ${14 * zoom}px`,
        borderRadius: 12 * zoom,
        borderWidth: zoom,
        borderColor: selected ? "var(--accent-blue)" : "var(--border)",
        opacity: selected ? 1 : 0.7,
        boxShadow: selected ? `0 0 0 ${2 * zoom}px var(--accent-blue)` : undefined,
        fontSize: 13 * zoom,
      }}
    >
      <div className="flex items-center" style={{ gap: 8 * zoom }}>
        <Icon
          className={`shrink-0 ${iconColor}`}
          style={{ width: 16 * zoom, height: 16 * zoom }}
        />
        <span
          className="flex-1 truncate font-semibold text-[var(--text-secondary)]"
          style={{ fontSize: 13 * zoom }}
        >
          {node.name}
        </span>
      </div>
      {node.description && (
        <div
          className="leading-snug text-[var(--text-muted)]"
          style={{ marginTop: 4 * zoom, fontSize: 11 * zoom }}
        >
          {node.description}
        </div>
      )}
    </div>
  );
}
