/**
 * The problem, before any of the app: sessions' tool calls pouring past
 * faster than anyone reads, then the summary that comes back as an essay.
 * The essay closes in on the middle of the stage and condenses into the
 * mark: the picture the title then names.
 */

import { Img, staticFile } from "remotion";
import { SANS, mix, p, s } from "../../kit";
import { CardTitle, Dock, Fades, Field, Stage2, cardStyle } from "../dressing";
import { Waterfall, shown, win } from "../figures";

export const PROBLEM2 = s(13.2);

const K = { clear: s(5.4), essay: s(5.6), collapse: s(11.2) };

/** Where the mark lands, matching the title's first frame. */
const MARK = { x: 960, y: 500, size: 184 };

/** The parts of the summary that change the architecture or the evidence, in reading order. */
const BURIED = ["while keeping the original refund method in place", "I added an in-memory entry store under domain/", "I also refactored the webhook retry logic", "Two tests are skipped for now"];

const ESSAY = [
  "Done! Here's a summary of what I changed.",
  "I implemented partial refunds across the ledger and the gateway. The main change is in ledger.rs, where I added refund_partial_v2 to track the remaining amount, {while keeping the original refund method in place} for backwards compatibility with existing callers.",
  "To make testing easier {I added an in-memory entry store under domain/}, and updated the facade so new callers can opt into the v2 path. The gateway now forwards an optional amount field.",
  "While I was there, {I also refactored the webhook retry logic}, since refunds emit events, and tightened some types in the dashboard's refund form. {Two tests are skipped for now} because they depend on the settlement file format.",
  "The old refund path should be removed in a follow-up once all callers are migrated. Let me know if you'd like me to add a feature flag, more tests, or documentation for the new endpoint.",
];

export function Problem2({ f }: { f: number }) {
  const essay = win(f, K.essay, PROBLEM2 + s(1));
  const scroll = Math.max(0, f - K.essay - s(1.0)) * 2.2;
  const top = 120 - scroll;
  const c = p(f, K.collapse, s(1.1), "move");
  const mark = p(f, K.collapse + s(0.45), s(0.8));
  return (
    <Stage2>
      <Field opacity={1 - p(f, K.clear, s(0.8), "move")}>
        <Waterfall f={f} />
      </Field>
      <div style={{ position: "absolute", left: 0, right: 0, top: 470, display: "flex", justifyContent: "center", zIndex: 6, ...cardStyle(f, K.clear) }}>
        <CardTitle f={f} lines={["Agents write faster than you can read."]} size={100} />
      </div>
      {essay > 0 ? (
        <div
          style={{
            position: "absolute",
            left: 300,
            width: 1320,
            top,
            zIndex: 3,
            fontFamily: SANS,
            fontSize: 40,
            lineHeight: 1.5,
            fontWeight: 420,
            letterSpacing: "-0.01em",
            color: "rgba(242,243,245,0.34)",
            ...shown(essay * (1 - c * c)),
            // Closes in on the mark's centre.
            transformOrigin: `${MARK.x - 300}px ${MARK.y - top}px`,
            transform: `scale(${mix(1, 0.04, c)})`,
            filter: c > 0.01 ? `blur(${c * 6}px)` : undefined,
          }}
        >
          {ESSAY.map((para, i) => (
            <p key={i} style={{ margin: "0 0 30px" }}>
              {para.split(/(\{[^}]+\})/).map((part, j) => {
                if (!part.startsWith("{")) return <span key={j}>{part}</span>;
                const n = BURIED.indexOf(part.slice(1, -1));
                const t = p(f, K.essay + s(1.6) + n * s(1.1), s(0.5));
                return (
                  <span key={j} style={{ color: `rgba(240,180,76,${0.34 + 0.66 * t})`, boxShadow: `inset 0 -${3 * t}px 0 rgba(240,180,76,${0.8 * t})` }}>
                    {part.slice(1, -1)}
                  </span>
                );
              })}
            </p>
          ))}
        </div>
      ) : null}
      {essay > 0 && c < 1 ? <Fades /> : null}
      <Dock f={f} at={K.essay + s(0.6)} until={K.collapse - s(0.2)} text="You asked for a change. You got an essay." subs={[{ at: K.essay + s(1.6), text: "What matters is buried in it." }]} />
      {mark > 0 ? (
        <Img
          src={staticFile("logo.png")}
          style={{ position: "absolute", left: MARK.x - MARK.size / 2, top: MARK.y - MARK.size / 2, width: MARK.size, height: MARK.size, zIndex: 7, opacity: mark, transform: `scale(${mix(0.35, 1, mark)})`, filter: mark < 1 ? `blur(${(1 - mark) * 8}px)` : undefined }}
        />
      ) : null}
    </Stage2>
  );
}
