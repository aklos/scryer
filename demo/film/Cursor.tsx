/**
 * The pointer, a sprite the script drives: it glides between targets and
 * rings once where it clicks. A target names an element of the mounted app
 * (a claim row, the concern lens) and is measured once the app has settled
 * for the frame, so the pointer lands on the real UI wherever it is laid out.
 */

import { useLayoutEffect, useState } from "react";
import { continueRender, delayRender, interpolate, useCurrentFrame } from "remotion";
import { appSettled } from "../stub/backend";
import type { CursorPoint, CursorState } from "./state";

/** The film's own frame, the box every position is measured against. */
export const FILM_ROOT = "data-film-root";

/** A point in film pixels: the centre of its target when the app shows it. */
function resolve(p: CursorPoint): { x: number; y: number } {
  const root = document.querySelector(`[${FILM_ROOT}]`);
  const el = p.target ? document.querySelector(p.target) : null;
  if (!root || !el) return { x: p.x, y: p.y };
  const box = root.getBoundingClientRect();
  // Studio scales the frame to fit the preview; a render does not.
  const scale = box.width / 1920;
  const r = el.getBoundingClientRect();
  return { x: (r.left + r.width / 2 - box.left) / scale, y: (r.top + r.height / 2 - box.top) / scale };
}

export function Cursor({ cursor }: { cursor: CursorState }) {
  const frame = useCurrentFrame();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  useLayoutEffect(() => {
    if (!cursor.visible) return;
    const handle = delayRender("cursor finds its target");
    void appSettled().then(() => {
      const a = resolve(cursor.from);
      const b = resolve(cursor.to);
      setAt({ x: a.x + (b.x - a.x) * cursor.t, y: a.y + (b.y - a.y) * cursor.t });
      continueRender(handle);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame, cursor.visible]);
  if (!cursor.visible || !at) return null;
  const since = cursor.clickAt === undefined ? -1 : frame - cursor.clickAt;
  const ring = since >= 0 && since < 14 ? since / 14 : null;
  const press = since >= 0 && since < 5 ? 0.88 : 1;
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y, pointerEvents: "none" }}>
      {ring !== null && (
        <div
          style={{
            position: "absolute",
            left: -22,
            top: -22,
            width: 44,
            height: 44,
            borderRadius: 9999,
            border: "2px solid rgba(96, 165, 250, 0.9)",
            transform: `scale(${interpolate(ring, [0, 1], [0.4, 1.3])})`,
            opacity: 1 - ring,
          }}
        />
      )}
      <svg
        width="26"
        height="30"
        viewBox="0 0 26 30"
        style={{ transform: `scale(${press})`, transformOrigin: "2px 2px", filter: "drop-shadow(0 2px 4px rgba(0,0,0,0.5))" }}
      >
        <path d="M2 2 L2 24 L8 18.5 L12.5 28 L16.5 26.2 L12 17 L20 17 Z" fill="#fff" stroke="#111" strokeWidth="1.6" strokeLinejoin="round" />
      </svg>
    </div>
  );
}
