/**
 * The close: no new ritual and no detour from how you already work with
 * your agent; the harnesses it sets itself up for; the mark and where to
 * get it.
 */

import { Img, staticFile } from "remotion";
import { ACCENT, GREEN, MONO, SANS, mix, p, s } from "../../kit";
import { CardTitle, DIM, INK, Stage2, cardStyle } from "../dressing";

export const CLOSE2 = s(11);

const HARNESSES = ["Claude Code", "Codex", "Copilot CLI", "any MCP client"];
const MARK = s(5.8);

export function Close2({ f }: { f: number }) {
  const mark = p(f, MARK + s(0.1), s(1.0));
  return (
    <Stage2>
      {f < MARK + s(0.4) && (
        <div style={{ position: "absolute", left: 0, right: 0, top: 300, display: "flex", flexDirection: "column", alignItems: "center", ...cardStyle(f, MARK - s(0.4)) }}>
          <CardTitle f={f} lines={["No rituals. No detours."]} />
          <div style={{ marginTop: 30, fontFamily: SANS, fontSize: 44, fontWeight: 460, letterSpacing: "-0.02em", color: DIM, opacity: p(f, s(1.1), s(0.8)) }}>
            Keep the agent you use, and the way you use it.
          </div>
          <div style={{ marginTop: 70, display: "flex", gap: 64 }}>
            {HARNESSES.map((h, i) => {
              const t = p(f, s(2.0) + i * 5, s(0.6));
              const any = i === HARNESSES.length - 1;
              return (
                <span key={h} style={{ fontFamily: MONO, fontSize: 30, color: any ? DIM : INK, opacity: t, transform: `translateY(${(1 - t) * 12}px)` }}>
                  {h}
                </span>
              );
            })}
          </div>
          <div style={{ marginTop: 64, fontFamily: MONO, fontSize: 32, color: INK, opacity: p(f, s(3.1), s(0.6)), padding: "14px 28px", border: "1px solid rgba(255,255,255,0.16)", borderRadius: 12 }}>
            <span style={{ color: GREEN }}>$ </span>scryer-mcp setup on
          </div>
        </div>
      )}
      {f >= MARK && (
        <>
          <Img src={staticFile("logo.png")} style={{ position: "absolute", left: 960 - 330, top: 460 - 84, width: 168, height: 168, opacity: mark, transform: `scale(${mix(0.88, 1, mark)})`, filter: mark < 1 ? `blur(${(1 - mark) * 10}px)` : undefined }} />
          <div style={{ position: "absolute", left: 960 - 130, top: 460 - 72, fontFamily: SANS, fontSize: 128, fontWeight: 580, letterSpacing: "-0.05em", lineHeight: 1, color: INK, clipPath: `inset(-20px ${(1 - p(f, MARK + s(0.4), s(0.9))) * 100}% -20px 0)` }}>
            scryer
          </div>
          <div style={{ position: "absolute", left: 0, right: 0, top: 650, textAlign: "center", fontFamily: MONO, fontSize: 32, color: ACCENT, opacity: p(f, MARK + s(1.2), s(0.8)) }}>
            github.com/aklos/scryer
          </div>
        </>
      )}
    </Stage2>
  );
}
