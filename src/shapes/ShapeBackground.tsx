import type { C4Shape } from "../types";
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
}

export function ShapeBackground({
  shape,
  fillClass,
  strokeClass,
  strokeWidth = 0,
  strokeDasharray,
  opacity,
}: Props) {
  return (
    <svg
      className="absolute inset-0 pointer-events-none overflow-visible"
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      style={opacity != null && opacity < 1 ? { opacity, isolation: "isolate" } : undefined}
    >
      <ShapePaths
        shape={shape}
        fillClass={fillClass}
        strokeClass={strokeClass}
        strokeWidth={strokeWidth}
        strokeDasharray={strokeDasharray}
        skipBaseFill={opacity != null && opacity < 1}
      />
    </svg>
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
          <path d={b.bottomCapPath} {...outline} />
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
