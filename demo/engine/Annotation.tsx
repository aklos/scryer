/**
 * Anchored annotation — a focal pin + a coachmark label. A pulsing dot is
 * dropped on a corner of the target (the point the camera is on), with a short
 * leader out to a solid label tag set into open space. It lives in screen space
 * above the camera, so it keeps constant size while the content is zoomed; the
 * director places it during a hold, when the camera is settled and the target's
 * rect is stable.
 *
 * The look is deliberately UNLIKE the app: a saturated marker fill with dark ink,
 * the inverse of the app's dark surfaces + light text — so it reads as a
 * director's overlay drawn on top, never as another product panel.
 */

export interface Annotation {
  /** Target rect in frame-relative screen px. */
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  /** Where the pin + label sit relative to the target:
   *  - top/bottom: above the top / below the bottom, off the leading corner.
   *  - right: off the right edge, out into open space beside it.
   *  - above: centered over the target's top edge (a vertical leader). */
  place: "top" | "bottom" | "right" | "above";
  /** Set while the label is being dismissed — fades the whole mark out instead
   *  of letting it pop out of existence. */
  exiting?: boolean;
  /** Hard cap on the label's width (px), so it wraps rather than running past
   *  the camera frame. The director computes this from the room actually left
   *  to the frame edge. */
  maxWidth: number;
}

const LEAD = 60; // leader length to the label

export function AnnotationLayer({ data }: { data: Annotation | null }) {
  if (!data) return null;
  const { x, y, w, h, text, place, maxWidth, exiting } = data;

  // Pin point + leader end. `right` pins the right edge and runs straight out
  // into the gutter; `above` pins the top-centre and runs straight up; top/bottom
  // pin the leading corner and angle up/down-right.
  const onRight = place === "right";
  const above = place === "above";
  const px = above ? x + w / 2 : onRight ? x + w : x;
  const py = above ? y : onRight ? y + h / 2 : place === "bottom" ? y + h : y;
  const bx = above ? px : px + LEAD;
  const by = above ? py - LEAD : onRight ? py : place === "bottom" ? py + LEAD * 0.62 : py - LEAD * 0.62;

  // The root is keyed by TEXT ONLY: a re-measure of the same annotation must
  // reposition it, never remount it — a remount restarts the entrance
  // animation, which reads as a jitter. New text = new annotation, fresh anim.
  return (
    <div className="film-annot" data-exiting={exiting ? "true" : "false"} key={text}>
      {/* The leader, in raw frame px (no viewBox), so it lands exactly. */}
      <svg className="film-mark-svg" aria-hidden>
        <path className="film-mark-leader" d={`M${px},${py} L${bx},${by}`} pathLength={100} />
      </svg>

      {/* The focal pin — a solid dot on the target edge. */}
      <span className="film-mark-pin" style={{ left: px, top: py }}>
        <span className="film-mark-pin-dot" />
      </span>

      <div
        className={`film-mark-bubble film-mark-bubble--${place}`}
        style={{ left: bx + (onRight ? 6 : 0), top: by, maxWidth }}
      >
        {text}
      </div>
    </div>
  );
}
