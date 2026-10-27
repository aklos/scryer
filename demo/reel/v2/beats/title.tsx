/**
 * The name: the mark, already there from the essay that condensed into it,
 * moves aside; the wordmark opens beside it, the line under.
 */

import { Img, staticFile } from "remotion";
import { SANS, mix, p, s } from "../../kit";
import { DIM, INK, Stage2 } from "../dressing";

export const TITLE2 = s(4.6);

export function Title2({ f }: { f: number }) {
  const cx = 960;
  const cy = 500;
  const slide = p(f, s(0.5), s(0.9), "move");
  const word = p(f, s(0.8), s(0.9));
  const line = p(f, s(1.5), s(0.9));
  const markX = mix(cx, cx - 250, slide);
  return (
    <Stage2>
      <Img src={staticFile("logo.png")} style={{ position: "absolute", left: markX - 92, top: cy - 92, width: 184, height: 184 }} />
      <div style={{ position: "absolute", left: cx - 110, top: cy - 80, fontFamily: SANS, fontSize: 140, fontWeight: 580, letterSpacing: "-0.05em", lineHeight: 1, color: INK, clipPath: `inset(-20px ${(1 - word) * 100}% -20px 0)`, opacity: word }}>
        scryer
      </div>
      <div style={{ position: "absolute", left: cx - 104, top: cy + 92, fontFamily: SANS, fontSize: 42, fontWeight: 460, letterSpacing: "-0.02em", color: DIM, opacity: line, transform: `translateY(${(1 - line) * 12}px)` }}>
        What's true about your codebase.
      </div>
    </Stage2>
  );
}
