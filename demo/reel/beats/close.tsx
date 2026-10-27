/**
 * The close: no new ritual and no detour from how you already work with
 * your agent; the harnesses it sets itself up for; the mark and where to
 * get it.
 */

import { Img, staticFile } from "remotion";
import { LIGHT, MONO, SANS, Stage, Statement, mix, p, s } from "../kit";

export const CLOSE = s(10);
const T = LIGHT;

const HARNESSES = ["Claude Code", "Codex", "Copilot CLI", "any MCP client"];
const MARK = s(5.2);

export function Close({ f }: { f: number }) {
  const out = p(f, MARK - s(0.4), s(0.6), "move");
  const mark = p(f, MARK + s(0.1), s(1.0));
  return (
    <Stage theme={T}>
      {f < MARK + s(0.4) && (
        <div style={{ position: "absolute", inset: 0, opacity: 1 - out, filter: out > 0 ? `blur(${out * 8}px)` : undefined }}>
          <Statement f={f} theme={T} index="" text="No rituals. No detours." dockAt={s(99)} />
          <div style={{ position: "absolute", left: 150, top: 560, fontFamily: SANS, fontSize: 42, fontWeight: 460, letterSpacing: "-0.02em", color: T.dim, opacity: p(f, s(1.0), s(0.8)) }}>
            Keep the agent you use, and the way you use it.
          </div>
          <div style={{ position: "absolute", left: 150, top: 680, display: "flex", gap: 40 }}>
            {HARNESSES.map((h, i) => {
              const t = p(f, s(1.9) + i * 5, s(0.6));
              return (
                <span key={h} style={{ display: "flex", alignItems: "center", gap: 14, opacity: t, transform: `translateY(${(1 - t) * 12}px)` }}>
                  <span style={{ width: 11, height: 11, transform: "rotate(45deg)", background: i === 3 ? "transparent" : T.text, border: i === 3 ? `1.5px solid ${T.dim}` : undefined }} />
                  <span style={{ fontFamily: MONO, fontSize: 26, color: i === 3 ? T.dim : T.text }}>{h}</span>
                </span>
              );
            })}
          </div>
          <div style={{ position: "absolute", left: 150, top: 780, fontFamily: MONO, fontSize: 24, color: T.dim, opacity: p(f, s(3.0), s(0.6)) }}>
            $ scryer-mcp setup on
          </div>
        </div>
      )}
      {f >= MARK && (
        <>
          <Img
            src={staticFile("logo.png")}
            style={{ position: "absolute", left: 960 - 330, top: 460 - 84, width: 168, height: 168, opacity: mark, transform: `scale(${mix(0.85, 1, mark)})`, filter: mark < 1 ? `blur(${(1 - mark) * 10}px)` : undefined }}
          />
          <div
            style={{
              position: "absolute",
              left: 960 - 130,
              top: 460 - 72,
              fontFamily: SANS,
              fontSize: 128,
              fontWeight: 580,
              letterSpacing: "-0.05em",
              lineHeight: 1,
              color: T.text,
              clipPath: `inset(-20px ${(1 - p(f, MARK + s(0.4), s(0.9))) * 100}% -20px 0)`,
            }}
          >
            scryer
          </div>
          <div style={{ position: "absolute", left: 0, right: 0, top: 640, textAlign: "center", fontFamily: MONO, fontSize: 26, color: T.dim, opacity: p(f, MARK + s(1.2), s(0.8)) }}>
            github.com/aklos/scryer
          </div>
        </>
      )}
    </Stage>
  );
}
