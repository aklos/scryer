/**
 * Camera geometry. The "camera" is a CSS transform on the content layer:
 * `translate(x, y) scale(s)` with transform-origin 0 0. Given a region we want
 * to frame, `frameTransform` solves for the (x, y, s) that centers that region
 * in the viewport with breathing room. `contentRect` recovers a target's
 * position in untransformed content space, so framing composes regardless of
 * where the camera currently sits.
 */

export interface Transform {
  x: number;
  y: number;
  s: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface FrameOpts {
  /** Breathing room around the target, in content px. */
  pad?: number;
  /** Force a specific scale instead of fitting the target. */
  zoom?: number;
  /** Never push in past this. Keeps text from getting gauzy. */
  maxZoom?: number;
  /** Never pull out past this. Defaults to 1 — at 1 a full-bleed scene exactly
   *  fills the frame, and zooming out would letterbox it. Scenes that render
   *  their UI on a larger canvas (so there's real background to reveal) pass a
   *  lower floor to frame a region wider than the viewport. */
  minZoom?: number;
}

export const IDENTITY: Transform = { x: 0, y: 0, s: 1 };

/**
 * A target's rect in untransformed content coordinates. Both rects come back
 * already scaled by whatever the camera is currently doing; dividing by the
 * live scale cancels it out, leaving stable content-space geometry.
 */
export function contentRect(target: HTMLElement, content: HTMLElement): Rect {
  const c = content.getBoundingClientRect();
  const t = target.getBoundingClientRect();
  const scale = c.width / content.offsetWidth || 1;
  return {
    x: (t.left - c.left) / scale,
    y: (t.top - c.top) / scale,
    w: t.width / scale,
    h: t.height / scale,
  };
}

/** The transform that frames `rect` within a `view`-sized viewport. */
export function frameTransform(
  rect: Rect,
  view: { w: number; h: number },
  opts: FrameOpts = {},
): Transform {
  const pad = opts.pad ?? 48;
  const fit = Math.min(
    view.w / (rect.w + 2 * pad),
    view.h / (rect.h + 2 * pad),
  );
  // Clamp the scale: never past maxZoom (softens the UI), never below minZoom
  // (defaults to 1 — letterboxes a full-bleed scene; lower it on a canvas scene).
  const s = Math.max(opts.minZoom ?? 1, Math.min(opts.zoom ?? fit, opts.maxZoom ?? 2.6));
  const cx = rect.x + rect.w / 2;
  const cy = rect.y + rect.h / 2;
  return { x: view.w / 2 - cx * s, y: view.h / 2 - cy * s, s };
}
