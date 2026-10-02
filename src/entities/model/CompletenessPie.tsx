import type { Completeness } from "./health";

/** SVG path for a pie wedge from 12 o'clock, sweeping clockwise to `pct`%. */
export function wedgePath(k: number, r: number, pct: number): string {
  if (pct >= 100) {
    // A single arc can't close a full circle — draw it as two half-arcs.
    return `M${k},${k - r} A${r},${r} 0 1,1 ${k},${k + r} A${r},${r} 0 1,1 ${k},${k - r} Z`;
  }
  const theta = (pct / 100) * 2 * Math.PI;
  const x = k + r * Math.sin(theta);
  const y = k - r * Math.cos(theta);
  const large = pct > 50 ? 1 : 0;
  return `M${k},${k} L${k},${k - r} A${r},${r} 0 ${large},1 ${x},${y} Z`;
}

/** Completeness as a semi-filled pie: the wedge fills clockwise to the % of the
 *  node's authored claims that read through to code. Anchorage is the fill itself
 *  — an empty ring is "nothing built / not grounded"; a dashed ring means there
 *  is nothing to measure yet (a bare box with no leaf claims). */
export function CompletenessPie({ c, size = 14 }: { c: Completeness; size?: number }) {
  const K = 10; // center
  const R = 7;
  const measured = c.pct !== undefined;
  return (
    <svg width={size} height={size} viewBox="0 0 20 20">
      {/* --border is a panel-hairline tone (near-canvas), invisible at this
          size — the ring needs a text tone to read on both themes. */}
      <circle
        cx={K}
        cy={K}
        r={R}
        fill="none"
        stroke="var(--text-ghost)"
        strokeWidth="1.75"
        strokeDasharray={measured ? undefined : "2.5 2.5"}
      />
      {measured && c.pct! > 0 && (
        <path d={wedgePath(K, R, c.pct!)} fill="var(--text-tertiary)" />
      )}
    </svg>
  );
}
