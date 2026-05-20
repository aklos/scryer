/**
 * SVG shape paths for C4-style node decorations.
 */

/**
 * Native coordinate space of the person silhouette — head + shoulders.
 * The viewBox is sized so the arc's top extreme (~y=2 with the path below)
 * sits inside it with a hair of padding. The ellipse center is below the
 * chord, so the arc reaches considerably further above the chord line than
 * the chord-to-radius offset suggests.
 */
export const PERSON_VIEWBOX = { w: 180, h: 96 };

/**
 * C4 person silhouette — a head atop shoulders that flare outward, the same
 * actor decoration the 0.2.5 node renderer drew. With `closed` the bottom edge
 * is sealed (use for a fill); left open it is a bare outline (use for a stroke,
 * so it can fade out at the bottom rather than ending in a hard line).
 */
export function personPath(closed: boolean): string {
  const arc =
    "M 33,96 C 33,66 48,52 76,48" +
    " A 22,26 0 1,1 104,48" +
    " C 132,52 147,66 147,96";
  return closed ? `${arc} Z` : arc;
}
