import type { C4Kind, C4Shape } from "../types";
import {
  baseRectPath,
  rectanglePath,
  personPath,
  cylinderParts,
  pipeParts,
  trapezoidPath,
  bucketParts,
  hexagonPath,
} from "./paths";

const W = 180;
const H = 160;

interface Props {
  shape: C4Shape;
  fillClass: string;
  strokeClass?: string;
  strokeWidth?: number;
  strokeDasharray?: string;
  opacity?: number;
  kind?: C4Kind;
  external?: boolean;
  changed?: boolean;
}

export function ShapeBackground({
  shape,
  fillClass,
  strokeClass,
  strokeWidth = 0,
  strokeDasharray,
  opacity,
  kind,
  external,
  changed,
}: Props) {
  return (
    <svg
      className="absolute inset-0 pointer-events-none overflow-visible"
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      style={opacity != null && opacity < 1 ? { opacity, isolation: "isolate" } : undefined}
    >
      {kind === "container" && shape === "rectangle" && (
        <path
          d={`M0,0 V-9 Q0,-12 3,-12 H39 Q42,-12 44,-9 L45,0`}
          className={`${fillClass} ${strokeClass ?? ""}`}
          strokeWidth={strokeWidth}
          strokeDasharray={strokeDasharray}
        />
      )}
      <ShapePaths
        shape={shape}
        fillClass={fillClass}
        strokeClass={strokeClass}
        strokeWidth={strokeWidth}
        strokeDasharray={external ? undefined : strokeDasharray}
        skipBaseFill={opacity != null && opacity < 1}
      />
      {kind === "system" && shape === "rectangle" && (
        <rect
          x={-6} y={-6}
          width={W + 12} height={H + 12}
          rx={6}
          fill="none"
          className={strokeClass}
          strokeWidth={1}
          strokeDasharray={external ? "6 3" : undefined}
        />
      )}
      {kind === "component" && shape === "rectangle" && (
        <>
          <rect x={-8} y={47} width={12} height={20} rx={2}
            className={`${fillClass} ${strokeClass ?? ""}`}
            strokeWidth={strokeWidth} strokeDasharray={strokeDasharray} />
          <rect x={-8} y={93} width={12} height={20} rx={2}
            className={`${fillClass} ${strokeClass ?? ""}`}
            strokeWidth={strokeWidth} strokeDasharray={strokeDasharray} />
        </>
      )}
      {changed && (
        <GlowOverlay shape={shape} kind={kind} />
      )}
    </svg>
  );
}

/** Pulsing glow overlay that follows the shape contour */
function GlowOverlay({ shape, kind }: { shape: C4Shape; kind?: C4Kind }) {
  const glowProps = {
    fill: "none",
    stroke: "var(--color-violet-400)",
    strokeWidth: 3,
  };

  const paths = (() => {
    switch (shape) {
      case "rectangle":
        return <path d={rectanglePath(W, H)} {...glowProps} />;
      case "person":
        return <path d={personPath(W, H)} {...glowProps} />;
      case "cylinder": {
        const c = cylinderParts(W, H);
        return <><path d={c.bodyPath} {...glowProps} /><path d={c.topCapPath} {...glowProps} /></>;
      }
      case "pipe": {
        const p = pipeParts(W, H);
        return <><path d={p.bodyPath} {...glowProps} /><path d={p.rightCapPath} {...glowProps} /></>;
      }
      case "trapezoid":
        return <path d={trapezoidPath(W, H)} {...glowProps} />;
      case "bucket": {
        const b = bucketParts(W, H);
        return <><path d={b.bodyPath} {...glowProps} /><path d={b.topCapPath} {...glowProps} /></>;
      }
      case "hexagon":
        return <path d={hexagonPath(W, H)} {...glowProps} />;
    }
  })();

  return (
    <g className="scryer-changed-glow">
      {paths}
      {kind === "container" && shape === "rectangle" && (
        <path d={`M0,0 V-9 Q0,-12 3,-12 H39 Q42,-12 44,-9 L45,0`} {...glowProps} />
      )}
      {kind === "component" && shape === "rectangle" && (
        <>
          <rect x={-8} y={47} width={12} height={20} rx={2} {...glowProps} />
          <rect x={-8} y={93} width={12} height={20} rx={2} {...glowProps} />
        </>
      )}
      {kind === "system" && shape === "rectangle" && (
        <rect x={-6} y={-6} width={W + 12} height={H + 12} rx={6} {...glowProps} strokeWidth={2} />
      )}
    </g>
  );
}

function ShapePaths({
  shape,
  fillClass,
  strokeClass,
  strokeWidth,
  strokeDasharray,
  skipBaseFill,
}: {
  shape: C4Shape;
  fillClass: string;
  strokeClass?: string;
  strokeWidth?: number;
  strokeDasharray?: string;
  skipBaseFill?: boolean;
}) {
  // Base fill: sharp rect always covers the full content area
  // Skipped when semi-transparent to avoid overlap artifacts
  const baseFill = skipBaseFill ? null : <path d={baseRectPath(W, H)} className={fillClass} />;

  // Shape outline: stroke follows the shape contour
  const outline = {
    className: `${fillClass} ${strokeClass ?? ""}`.trim(),
    strokeWidth,
    strokeDasharray,
  };

  switch (shape) {
    case "rectangle":
      return <path d={rectanglePath(W, H)} {...outline} />;

    case "person":
      return <path d={personPath(W, H)} {...outline} />;

    case "cylinder": {
      const c = cylinderParts(W, H);
      return (
        <>
          {baseFill}
          <path d={c.bodyPath} {...outline} />
          <path d={c.topCapPath} {...outline} />
        </>
      );
    }

    case "pipe": {
      const p = pipeParts(W, H);
      return (
        <>
          {baseFill}
          <path d={p.bodyPath} {...outline} />
          <path d={p.rightCapPath} {...outline} />
        </>
      );
    }

    case "trapezoid":
      return (
        <>
          {baseFill}
          <path d={trapezoidPath(W, H)} {...outline} />
        </>
      );

    case "bucket": {
      const b = bucketParts(W, H);
      return (
        <>
          {baseFill}
          <path d={b.bodyPath} {...outline} />
          <path d={b.topCapPath} {...outline} />
        </>
      );
    }

    case "hexagon":
      return (
        <>
          {baseFill}
          <path d={hexagonPath(W, H)} {...outline} />
        </>
      );
  }
}
