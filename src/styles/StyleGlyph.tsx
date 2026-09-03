/**
 * The miniature of a style's drawing — the glyph that says what shape a
 * container's inside takes before anyone drills in: a hexagon, a stack of
 * bands, rings, or columns. Used as the container's icon in the tree and on
 * its diagram card.
 */

import type { ReactElement } from "react";

/** The miniature of a style's drawing — a container's card shows what shape
 *  its inside takes before anyone drills in: a hexagon, a stack of bands,
 *  rings, or columns. */
export function StyleGlyph({ style, className }: { style: string; className?: string }) {
  const stroke = "var(--text-tertiary)";
  const common = { fill: "none", stroke, strokeWidth: 1.4 };
  let body: ReactElement;
  switch (style) {
    case "hexagonal":
      body = <polygon points="7,1 12.2,4 12.2,10 7,13 1.8,10 1.8,4" {...common} />;
      break;
    case "feature-sliced":
      body = (
        <g {...common}>
          <rect x="1.5" y="1.5" width="11" height="2.6" rx="0.8" />
          <rect x="1.5" y="5.7" width="11" height="2.6" rx="0.8" />
          <rect x="1.5" y="9.9" width="11" height="2.6" rx="0.8" />
        </g>
      );
      break;
    case "core-shell":
      body = (
        <g {...common}>
          <circle cx="7" cy="7" r="5.8" />
          <circle cx="7" cy="7" r="2.4" />
        </g>
      );
      break;
    case "pipeline":
      body = (
        <g {...common}>
          <rect x="1.2" y="2" width="2.8" height="10" rx="0.8" />
          <rect x="5.6" y="2" width="2.8" height="10" rx="0.8" />
          <rect x="10" y="2" width="2.8" height="10" rx="0.8" />
        </g>
      );
      break;
    default:
      body = <rect x="1.5" y="1.5" width="11" height="11" rx="2" {...common} />;
  }
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" className={`shrink-0 overflow-visible ${className ?? ""}`} aria-hidden>
      {body}
    </svg>
  );
}

