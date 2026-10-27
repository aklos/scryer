/**
 * The name: the mark settles, the wordmark opens beside it, the line under.
 */

import { Img, staticFile } from "remotion";
import { LIGHT, SANS, Stage, mix, p, s } from "../kit";

export const TITLE = s(4.2);
const T = LIGHT;

export function Title({ f }: { f: number }) {
  const cx = 960;
  const cy = 520;
  const resolve = p(f, s(0.45), s(0.8));
  const slide = p(f, s(0.9), s(0.9), "move");
  const word = p(f, s(1.2), s(0.9));
  const line = p(f, s(1.9), s(0.9));
  const markX = mix(cx, cx - 250, slide);
  return (
    <Stage theme={T}>
      <Img
        src={staticFile("logo.png")}
        style={{
          position: "absolute",
          left: markX - 92,
          top: cy - 92,
          width: 184,
          height: 184,
          opacity: resolve,
          transform: `scale(${mix(0.85, 1, resolve)})`,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: cx - 110,
          top: cy - 80,
          fontFamily: SANS,
          fontSize: 140,
          fontWeight: 580,
          letterSpacing: "-0.05em",
          lineHeight: 1,
          color: T.text,
          clipPath: `inset(-20px ${(1 - word) * 100}% -20px 0)`,
          opacity: word,
        }}
      >
        scryer
      </div>
      <div
        style={{
          position: "absolute",
          left: cx - 104,
          top: cy + 84,
          fontFamily: SANS,
          fontSize: 36,
          fontWeight: 460,
          letterSpacing: "-0.02em",
          color: T.dim,
          opacity: line,
          transform: `translateY(${(1 - line) * 12}px)`,
        }}
      >
        What's true about your codebase.
      </div>
    </Stage>
  );
}
