/**
 * Perimeter nodes — actors and reference cards that live in the ring bands
 * around the inner grid. All dimensions scale by `useZoom()` (figma-style).
 */

import { useContext, useId } from "react";
import type { ReactNode } from "react";
import type { Entry } from "./viewmodel";
import { personPath, PERSON_VIEWBOX } from "./shapes";
import { ModelContext } from "./modelcontext";
import { useZoom } from "./PanZoom";
import { effectiveEntryStatus } from "./rollup";
import { STATUS_COLORS } from "./statusColors";
import { tokenIcon } from "./tokens";

export const PERSON_W = 190;
export const PERSON_SVG_H = (PERSON_VIEWBOX.h / PERSON_VIEWBOX.w) * PERSON_W;

function PersonShape({ children }: { children: ReactNode }) {
  const zoom = useZoom();
  const uid = useId();
  const fillId = `person-fill-${uid}`;
  const strokeId = `person-stroke-${uid}`;
  const w = PERSON_W * zoom;
  const h = PERSON_SVG_H * zoom;
  return (
    <div className="flex flex-col items-center" style={{ width: w }}>
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
          stroke={`url(#${strokeId})`}
          strokeWidth={1.5}
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
  entry,
  variant,
}: {
  entry: Entry;
  variant: PerimeterVariant;
}) {
  const zoom = useZoom();
  const surfaces = useContext(ModelContext);

  if (variant === "person") {
    return (
      <PersonShape>
        <div
          className="truncate font-semibold text-[var(--text-secondary)]"
          style={{ fontSize: 13 * zoom }}
        >
          {entry.title}
        </div>
        {entry.description && (
          <div
            className="leading-snug text-[var(--text-muted)]"
            style={{ marginTop: 2 * zoom, fontSize: 11 * zoom }}
          >
            {entry.description}
          </div>
        )}
      </PersonShape>
    );
  }

  // External or in-scope reference — a quiet card: solid border, icon +
  // title + description. No tech label and no "REFERENCE" tag — the level
  // band already tells you what they are.
  const Icon = tokenIcon(entry.id);
  const eff = effectiveEntryStatus(surfaces, entry);
  const iconColor =
    variant === "external" || !eff
      ? "text-[var(--text-muted)]"
      : STATUS_COLORS[eff].icon;

  return (
    <div
      className="border border-[var(--border)] bg-[var(--surface-canvas)] opacity-70"
      style={{
        width: PERSON_W * zoom,
        padding: `${12 * zoom}px ${14 * zoom}px`,
        borderRadius: 12 * zoom,
        borderWidth: zoom,
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
          {entry.title}
        </span>
      </div>
      {entry.description && (
        <div
          className="leading-snug text-[var(--text-muted)]"
          style={{ marginTop: 4 * zoom, fontSize: 11 * zoom }}
        >
          {entry.description}
        </div>
      )}
    </div>
  );
}
