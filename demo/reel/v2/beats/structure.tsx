/**
 * Read the structure, not the code. The card sets the headline inside the
 * core of a hexagonal container. Then the app: every container drawn in its
 * declared style — Ledger's hexagon, Fraud's core and shell, Dashboard's
 * slices — and Ledger's claims read across one concern.
 */

import { AppWindow, type Cam, type Rect } from "../../AppWindow";
import { DARK, keyed, mix, p, s } from "../../kit";
import { nodeId } from "../../live/world";
import type { Navigate } from "../../live/backend";
import { CardTitle, Dock, Field, HexRings, Lift, PANEL_RIGHT, Stage2, cardStyle } from "../dressing";
import { win } from "../figures";
import { BASE2, NoTree } from "../world";

/** A map in the panel: the app fits it to the window; wide maps show the whole window, Ledger's hexagon a little closer. */
const MAP = { x: 800, y: 490, s: 0.78 };
const HEX = { x: 800, y: 470, s: 1.05 };

export const STRUCTURE2 = s(22);

const K = { clear: s(3.0), app: s(3.3), ledger: s(6.0), fraud: s(10.0), dash: s(12.4), page: s(14.8), lens: s(16.8) };

function navAt(f: number): Navigate {
  const map = (focus: string): Navigate => ({ view: "diagram", diagramFocus: nodeId(focus), lens: null });
  if (f < K.ledger) return map("Aperture Pay");
  if (f < K.fraud) return map("Ledger");
  if (f < K.dash) return map("Fraud");
  if (f < K.page) return map("Dashboard");
  return { view: "wiki", page: nodeId("Ledger"), lens: f >= K.lens ? "persistence" : null };
}

const LEDGER = `.react-flow__node[data-id="${nodeId("Ledger")}"]`;

export function Structure2({ f }: { f: number }) {
  const app = win(f, K.app, STRUCTURE2 + s(1), s(0.8));
  const aside = p(f, K.clear, s(1.2), "move");
  const push = s(0.8);
  const cam = (at: Partial<Record<"rows" | "ledger", Rect>>): Cam => {
    const view = MAP;
    const led = at.ledger;
    const r = at.rows;
    const z = 1.45;
    const rows = r ? { x: r.x - 70 + PANEL_RIGHT.w / 2 / z, y: r.y + 220, s: z } : { x: 900, y: 400, s: z };
    return keyed<Cam>(f, { ...view, rx: 0, ry: 0 }, [
      { at: K.ledger - push, dur: push, x: led?.cx, y: led?.cy, s: 3.0 },
      { at: K.ledger, dur: 1, ...HEX },
      { at: K.fraud, dur: 1, ...MAP },
      { at: K.page, dur: 1, ...rows },
      // The lens counts every node's claims on the concern, down the tree.
      { at: K.lens + s(0.3), dur: s(1.4), x: 0, y: 380 },
    ]);
  };
  return (
    <Stage2>
      {/* The rings stay as the beat's ground, drawn back to the left of the panel. */}
      <Field opacity={1 - 0.35 * aside}>
        <HexRings f={Math.min(f, K.clear + s(1.2))} cx={mix(960, 330, aside)} cy={mix(540, 420, aside)} core={mix(540, 250, aside)} />
      </Field>
      <div style={{ position: "absolute", left: 0, right: 0, top: 420, display: "flex", flexDirection: "column", alignItems: "center", zIndex: 6, ...cardStyle(f, K.clear) }}>
        <CardTitle f={f} lines={["Read the structure,", "not the code."]} />
      </div>
      {app > 0 ? (
        <div style={{ position: "absolute", inset: 0, opacity: app, transform: `translateY(${(1 - app) * 24}px)` }}>
          <Lift>
            <AppWindow f={f} theme={DARK} fixture={BASE2} version={0} nav={navAt(f)} cam={cam} frame={PANEL_RIGHT} targets={{ rows: { sel: '[id^="resp-"]', all: true }, ledger: { sel: LEDGER } }} />
            {navAt(f).view === "diagram" ? <NoTree /> : null}
          </Lift>
        </div>
      ) : null}
      <Dock
        f={f}
        at={K.app + s(0.4)}
        text="Read the structure, not the code."
        subs={[
          { at: K.app, text: "Every container, drawn in the style it declares." },
          { at: K.page, text: "Every claim is one sentence of the same shape." },
          { at: K.lens, text: "Pick a concern and follow it down the tree." },
        ]}
      />
    </Stage2>
  );
}
