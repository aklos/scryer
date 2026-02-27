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
  active?: boolean;
  onClick?: () => void;
}

export function ShapeIcon({ shape, active, onClick }: Props) {
  return (
    <button
      type="button"
      className={`flex items-center justify-center w-9 h-7 rounded border cursor-pointer transition-colors ${
        active
          ? "border-blue-400 bg-blue-50 dark:bg-blue-900/30 dark:border-blue-500"
          : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-600 dark:bg-zinc-700 dark:hover:border-zinc-500"
      }`}
      onClick={onClick}
      title={shape}
    >
      <svg
        width={24}
        height={19}
        viewBox={`-30 -70 ${W + 60} ${H + 80}`}
        className="pointer-events-none"
      >
        <IconPaths shape={shape} active={active} />
      </svg>
    </button>
  );
}

function IconPaths({ shape, active }: { shape: C4Shape; active?: boolean }) {
  const fillClass = active
    ? "fill-blue-200 dark:fill-blue-800"
    : "fill-zinc-100 dark:fill-zinc-600";
  const strokeClass = active
    ? "stroke-blue-500 dark:stroke-blue-400"
    : "stroke-zinc-400 dark:stroke-zinc-500";

  const baseFill = <path d={baseRectPath(W, H)} className={fillClass} />;
  const outline = {
    className: `${fillClass} ${strokeClass}`,
    strokeWidth: 6,
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
