/**
 * The pointer, a sprite the script drives: it glides between positions and
 * rings once where it clicks.
 */

import { interpolate, useCurrentFrame } from "remotion";
import type { CursorState } from "./state";

export function Cursor({ cursor }: { cursor: CursorState }) {
  const frame = useCurrentFrame();
  if (!cursor.visible) return null;
  const since = cursor.clickAt === undefined ? -1 : frame - cursor.clickAt;
  const ring = since >= 0 && since < 14 ? since / 14 : null;
  const press = since >= 0 && since < 5 ? 0.88 : 1;
  return (
    <div style={{ position: "absolute", left: cursor.x, top: cursor.y, pointerEvents: "none" }}>
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
