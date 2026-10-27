/**
 * Get caught up with an existing codebase: the map from the whole system
 * down to one container's components, that container's page, then a
 * question to the agent whose answer walks the app through the pages it
 * names.
 */

import { AppWindow, WIDE, glide, type Box, type Cam } from "../AppWindow";
import { Caption, DARK, Pointer, Stage, Statement, keyed, p, s } from "../kit";
import { Terminal, type TLine } from "../Terminal";
import { BASE, nodeId } from "../live/world";
import type { Navigate } from "../live/backend";

export const CATCHUP = s(21);
const T = DARK;

const K = {
  enter: s(1.9),
  sys: s(4.6),
  ledger: s(7.4),
  wiki: s(10.0),
  ask: s(12.2),
};

function navAt(f: number): Navigate {
  const none = { lens: null };
  if (f < K.sys) return { ...none, view: "diagram", diagramFocus: null };
  if (f < K.ledger) return { ...none, view: "diagram", diagramFocus: nodeId("Aperture Pay") };
  if (f < K.wiki) return { ...none, view: "diagram", diagramFocus: nodeId("Ledger") };
  if (f < K.ask + s(2.4)) return { ...none, view: "wiki", page: nodeId("Ledger") };
  if (f < K.ask + s(3.3)) return { ...none, view: "wiki", page: nodeId("API Gateway") };
  if (f < K.ask + s(4.2)) return { ...none, view: "wiki", page: nodeId("Ledger/application/facade") };
  return { ...none, view: "wiki", page: nodeId("Ledger/domain/refund") };
}

const LINES: TLine[] = [
  { at: K.ask + s(0.5), kind: "user", text: "how does a refund reach the bank?" },
  { at: K.ask + s(1.7), kind: "tool", name: "scryer · orient", arg: 'task: "refund"', done: K.ask + s(2.3) },
  { at: K.ask + s(2.4), kind: "out", text: "API Gateway — Routes a refund to the ledger" },
  { at: K.ask + s(3.3), kind: "out", text: "Ledger › application › facade" },
  { at: K.ask + s(4.2), kind: "out", text: "Ledger › domain › refund" },
  {
    at: K.ask + s(5.0),
    kind: "say",
    text: "The gateway hands the refund to Ledger's facade, which posts it through domain/refund once the refundable remainder checks out. The gateway then refunds with the acquiring bank.",
  },
];

// Window points measured in the live app (aperture-pay's layout): the
// Aperture Pay card on the context map, the Ledger card on the container
// map, the Wiki tab; and the boxes each map level's cards fill.
const AT = { system: [955, 449], ledger: [1057, 541], wiki: [1270, 17] } as const;

export function CatchUp({ f }: { f: number }) {
  const cam = keyed<Cam>(f, { x: 800, y: 500, s: 0.62, rx: 0, ry: 0 }, [
    { at: K.enter, dur: s(1.3), x: 800, y: 500, s: 0.84 },
    { at: s(3.2), dur: s(1.4), x: 966, y: 523, s: 1.08 },
    { at: K.sys, dur: s(1.0), x: 965, y: 523, s: 1.45 },
    { at: K.ledger, dur: s(1.0), x: 965, y: 523, s: 1.05 },
    { at: K.wiki - s(0.9), dur: s(0.9), x: 1000, y: 300, s: 1.15 },
    { at: K.wiki, dur: s(1.0), x: 900, y: 380, s: 1.2 },
    { at: K.ask, dur: s(1.0), x: 800, y: 400, s: 1.06 },
  ]);
  // The app makes room for the terminal when you ask.
  const fr = keyed<Box>(f, WIDE, [{ at: K.ask, dur: s(1.0), x: 80, w: 1000 }]);
  const win = p(f, K.enter, s(1.0));
  const termIn = p(f, K.ask, s(0.9));
  const ptr = glide(f, {}, [
    { at: s(3.2), to: [1250, 820] },
    { at: K.sys - s(0.25), to: [AT.system[0], AT.system[1]], click: true, dur: 34 },
    { at: K.ledger - s(0.25), to: [AT.ledger[0], AT.ledger[1]], click: true, dur: 40 },
    { at: K.wiki - s(0.25), to: [AT.wiki[0], AT.wiki[1]], click: true, dur: 40 },
  ]);
  const ptrShow = p(f, s(3.2), 8) * (1 - p(f, K.ask - s(0.6), 10));
  return (
    <Stage theme={T}>
      <AppWindow
        f={f}
        theme={T}
        fixture={BASE}
        version={0}
        nav={navAt(f)}
        cam={cam}
        frame={fr}
        opacity={win}
        overlay={() => <Pointer f={f} x={ptr.x} y={ptr.y} clickAt={ptr.clickAt} opacity={ptrShow} />}
      />
      {f >= K.ask && (
        <Terminal
          f={f}
          theme={T}
          title="~/aperture-pay — your agent"
          lines={LINES}
          width={720}
          height={840}
          style={{ left: 1120, top: 200, opacity: termIn, transform: `translateX(${(1 - termIn) * 80}px)` }}
        />
      )}
      <Statement f={f} theme={T} index="01" text="Get caught up with existing codebases." />
      <Caption f={f} theme={T} at={s(2.8)} until={K.wiki - s(0.2)}>
        The whole system as a map, derived from the code: from its context down to containers and components.
      </Caption>
      <Caption f={f} theme={T} at={K.wiki} until={K.ask - s(0.1)}>
        Every node has a page: what it is, the claims its tests prove, what it connects to.
      </Caption>
      <Caption f={f} theme={T} at={K.ask + s(0.2)} until={CATCHUP}>
        Ask your agent. It reads the same model, so its answer points at real places.
      </Caption>
    </Stage>
  );
}
