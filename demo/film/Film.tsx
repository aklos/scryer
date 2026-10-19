/**
 * The film composition: scryer on the left, the agent's terminal on the right
 * (a second one joins for the swarm act), captions and the cursor on top.
 * Everything comes from `state(frame)`; the camera is a transform on the
 * scryer window.
 */

import { useEffect, useState } from "react";
import { AbsoluteFill, continueRender, delayRender, useCurrentFrame } from "remotion";
import { Caption } from "./Caption";
import { Cursor, FILM_ROOT } from "./Cursor";
import { EVENTS } from "./events";
import { Shell } from "./Shell";
import { state } from "./state";
import { Terminal } from "./Terminal";

/** The scryer window and the terminal column, in film pixels. */
const WINDOW = { left: 40, top: 40, width: 1160, height: 1000 };
const TERMS = { left: 1240, top: 40, width: 640, height: 1000, gap: 20 };

/** Hold the first frame until the webfonts are in, so no frame renders in a fallback face. */
function useFonts() {
  const [handle] = useState(() => delayRender("webfonts"));
  useEffect(() => {
    document.fonts.ready.then(() => continueRender(handle));
  }, [handle]);
}

export function Film() {
  useFonts();
  const frame = useCurrentFrame();
  const s = state(EVENTS, frame);
  const [main, second] = s.terminals;
  const half = (TERMS.height - TERMS.gap) / 2;
  return (
    <AbsoluteFill
      {...{ [FILM_ROOT]: "" }}
      style={{ background: "radial-gradient(ellipse at 30% 20%, #1c2333 0%, #0b0d12 70%)" }}
    >
      <div
        style={{
          position: "absolute",
          ...WINDOW,
          overflow: "hidden",
          borderRadius: 12,
          border: "1px solid rgba(255,255,255,0.09)",
          boxShadow: "0 30px 80px rgba(0,0,0,0.55)",
        }}
      >
        <div
          style={{
            width: "100%",
            height: "100%",
            transformOrigin: `${s.camera.x}px ${s.camera.y}px`,
            transform: `scale(${s.camera.scale})`,
          }}
        >
          <Shell fixture={s.fixture} version={s.version} view={s.view} lens={s.lens} frame={frame} />
        </div>
      </div>
      <div style={{ position: "absolute", left: TERMS.left, top: TERMS.top, width: TERMS.width, height: second ? half : TERMS.height }}>
        <Terminal state={main} />
      </div>
      {second && (
        <div style={{ position: "absolute", left: TERMS.left, top: TERMS.top + half + TERMS.gap, width: TERMS.width, height: half }}>
          <Terminal state={second} />
        </div>
      )}
      <Cursor cursor={s.cursor} />
      <Caption caption={s.caption} />
    </AbsoluteFill>
  );
}
