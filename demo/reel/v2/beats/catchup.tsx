/**
 * Get caught up on any codebase. The app's own maps, in a panel: Aperture
 * Pay in its context, into its containers, into Ledger's hexagon, then
 * Ledger's page; then a question to the agent and the page its answer
 * lands on. Beside the panel, the beat's ground is the path taken: each
 * level drilled into, then the places the answer names.
 */

import { AppWindow, type Cam, type Rect } from "../../AppWindow";
import { ACCENT, DARK, MONO, SANS, keyed, mix, p, s } from "../../kit";
import { nodeId } from "../../live/world";
import type { Navigate } from "../../live/backend";
import { CardTitle, DIM, Dock, INK, Lift, PANEL_RIGHT, PanelFrame, Stage2, cardStyle } from "../dressing";
import { Term, win, type TLine } from "../figures";
import { BASE2, NoTree } from "../world";

export const CATCHUP2 = s(23);

const K = { clear: s(2.6), app: s(2.9), sys: s(6.2), led: s(9.6), page: s(13.0), ask: s(15.2), answer: s(20.2) };

/** The panel's segments: each mounts the app fresh, so a map is fitted when it appears. */
type Seg = { id: string; from: number; to: number; nav: Navigate; push?: string };
const SEGS: Seg[] = [
  { id: "context", from: K.app, to: K.sys, nav: { view: "diagram", diagramFocus: null, lens: null }, push: "Aperture Pay" },
  { id: "containers", from: K.sys, to: K.led, nav: { view: "diagram", diagramFocus: nodeId("Aperture Pay"), lens: null }, push: "Ledger" },
  { id: "ledger", from: K.led, to: K.page, nav: { view: "diagram", diagramFocus: nodeId("Ledger"), lens: null } },
  { id: "page", from: K.page, to: K.ask, nav: { view: "wiki", page: nodeId("Ledger"), lens: null } },
  { id: "answer", from: K.answer, to: CATCHUP2 + s(1), nav: { view: "wiki", page: nodeId("Ledger/domain/refund"), lens: null } },
];

/** A map shows the whole window (the app fits the diagram to it); a page its text. */
const MAP: Cam = { x: 800, y: 490, s: 0.78, rx: 0, ry: 0 };
const PAGE: Cam = { x: 400, y: 275, s: 1.5, rx: 0, ry: 0 };

const LINES: TLine[] = [
  { at: K.ask + s(0.3), kind: "user", text: "how does a refund reach the bank?" },
  { at: K.ask + s(1.9), kind: "tool", name: "scryer · orient", arg: 'task: "refund"' },
  { at: K.ask + s(2.5), kind: "out", text: "API Gateway — routes a refund to the ledger" },
  { at: K.ask + s(2.9), kind: "out", text: "Ledger › application › facade" },
  { at: K.ask + s(3.3), kind: "out", text: "Ledger › domain › refund" },
];

type Step = { at: number; name: string; note: string; glyph: "system" | "hex" | "box" };

/** The path down: each level as it is drilled into. */
const DOWN: Step[] = [
  { at: K.app, name: "Aperture Pay", note: "in its context", glyph: "system" },
  { at: K.sys, name: "Aperture Pay", note: "five containers", glyph: "system" },
  { at: K.led, name: "Ledger", note: "container · hexagonal", glyph: "hex" },
  { at: K.page, name: "Ledger", note: "its page · 24 claims", glyph: "hex" },
];

/** The answer's path, lit as orient names each place. */
const ANSWER: Step[] = [
  { at: K.ask + s(2.5), name: "API Gateway", note: "routes a refund to the ledger", glyph: "box" },
  { at: K.ask + s(2.9), name: "Ledger › application", note: "facade", glyph: "hex" },
  { at: K.ask + s(3.3), name: "Ledger › domain", note: "refund", glyph: "hex" },
];

function Glyph({ kind, hot }: { kind: Step["glyph"]; hot: boolean }) {
  const c = hot ? ACCENT : "#FFFFFF";
  const o = hot ? 1 : 0.45;
  return (
    <svg width={34} height={34} viewBox="0 0 34 34" style={{ flex: "none", marginTop: 8 }}>
      {kind === "hex" ? (
        <path d="M17 3 L29 10 L29 24 L17 31 L5 24 L5 10 Z" fill="none" stroke={c} strokeOpacity={o} strokeWidth={2} />
      ) : kind === "system" ? (
        <rect x={4} y={7} width={26} height={20} rx={4} fill="none" stroke={c} strokeOpacity={o} strokeWidth={2} />
      ) : (
        <rect x={7} y={9} width={20} height={16} rx={3} fill="none" stroke={c} strokeOpacity={o} strokeWidth={2} strokeDasharray="3 3" />
      )}
    </svg>
  );
}

/**
 * A path of steps down the left of the stage, joined by a rule: the newest
 * step is lit, the ones before it stay as where you came from. Steps that
 * repeat a name replace the one before rather than stacking.
 */
function Path({ f, steps, top, opacity }: { f: number; steps: Step[]; top: number; opacity: number }) {
  const seen = steps.filter((st) => f >= st.at);
  const rows: Step[] = [];
  for (const st of seen) {
    if (rows.length && rows[rows.length - 1].name === st.name) rows[rows.length - 1] = st;
    else rows.push(st);
  }
  const last = rows.length - 1;
  return (
    <div style={{ position: "absolute", left: 120, top, width: 480, opacity }}>
      {rows.map((st, i) => {
        const t = p(f, st.at, s(0.6));
        const hot = i === last;
        return (
          <div key={st.name} style={{ position: "relative", display: "flex", gap: 22, paddingBottom: 40, opacity: t, transform: `translateY(${(1 - t) * 14}px)` }}>
            {i < rows.length - 1 ? <div style={{ position: "absolute", left: 16, top: 50, bottom: 4, width: 2, background: "rgba(255,255,255,0.16)" }} /> : null}
            <Glyph kind={st.glyph} hot={hot} />
            <div>
              <div style={{ fontFamily: SANS, fontSize: 40, fontWeight: 560, letterSpacing: "-0.03em", color: hot ? INK : DIM, opacity: hot ? 1 : 0.7 }}>{st.name}</div>
              <div style={{ marginTop: 6, fontFamily: MONO, fontSize: 22, color: hot ? ACCENT : DIM, opacity: hot ? 1 : 0.6 }}>{st.note}</div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

export function CatchUp2({ f }: { f: number }) {
  const seg = SEGS.find((g) => f >= g.from - s(0.1) && f < g.to);
  const panel = p(f, K.app - s(0.3), s(0.8));
  const term = win(f, K.ask, K.answer, s(0.5));
  return (
    <Stage2>
      <div style={{ position: "absolute", left: 0, right: 0, top: 470, display: "flex", justifyContent: "center", zIndex: 6, ...cardStyle(f, K.clear) }}>
        <CardTitle f={f} lines={["Get caught up on any codebase."]} />
      </div>

      <Path f={f} steps={DOWN} top={90} opacity={1 - p(f, K.ask, s(0.5), "move")} />
      <Path f={f} steps={ANSWER} top={90} opacity={p(f, K.ask + s(2.2), s(0.4))} />

      {/* The panel's own ground, so a dip between segments falls to it, not to the stage. */}
      {panel > 0 ? <PanelFrame at={PANEL_RIGHT} opacity={panel} z={0} /> : null}

      {seg ? (
        <Segment key={seg.id} f={f} seg={seg} />
      ) : null}
      {term > 0 ? (
        <PanelFrame at={PANEL_RIGHT} opacity={term}>
          <Term f={f} lines={LINES} size={30} style={{ position: "absolute", left: 56, top: 64 }} />
        </PanelFrame>
      ) : null}

      <Dock
        f={f}
        at={K.clear + s(0.2)}
        text="Get caught up on any codebase."
        subs={[
          { at: K.app, text: "The whole system, as the code says it is." },
          { at: K.sys, text: "Its containers, each in the style it declares." },
          { at: K.led, text: "And inside each, its components." },
          { at: K.page, text: "Every node has a page: what it does, and what proves it." },
          { at: K.ask, text: "Ask your agent. It reads the same map." },
          { at: K.answer, text: "Its answer points at real places." },
        ]}
      />
    </Stage2>
  );
}

/**
 * One segment of the panel: it settles in from a little wide, holds, and
 * on a map pushes gently toward the card the next segment opens before
 * dipping out.
 */
function Segment({ f, seg }: { f: number; seg: Seg }) {
  const map = seg.nav.view === "diagram";
  const rest = map ? MAP : PAGE;
  const into = p(f, seg.from, s(0.5));
  const out = seg.push ? p(f, seg.to - s(0.45), s(0.4), "move") : 0;
  const o = into * (1 - out);
  const sel = seg.push ? `.react-flow__node[data-id="${nodeId(seg.push)}"]` : undefined;
  const cam = (at: Partial<Record<"push", Rect>>): Cam => {
    const tgt = at.push;
    return keyed<Cam>(f, { ...rest, s: rest.s * 0.94 }, [
      { at: seg.from, dur: s(0.8), s: rest.s },
      ...(tgt ? [{ at: seg.to - s(0.9), dur: s(0.9), x: mix(rest.x, tgt.cx, 0.7), y: mix(rest.y, tgt.cy, 0.7), s: rest.s * 1.45 }] : []),
    ]);
  };
  return (
    <div style={{ position: "absolute", inset: 0, opacity: o }}>
      <Lift>
        <AppWindow f={f} theme={DARK} fixture={BASE2} version={0} nav={seg.nav} cam={cam} frame={PANEL_RIGHT} targets={sel ? { push: { sel } } : undefined} />
        <NoTree />
      </Lift>
    </div>
  );
}
