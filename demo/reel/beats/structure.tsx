/**
 * Understand the structure without reading the code. C4 gives the
 * architecture both views: vertical, from the system down to components;
 * horizontal, every container drawn in its declared style and every
 * component carrying its layer. EARS gives the claims a horizontal view: one
 * sentence shape, read across every row. Concerns give the vertical view: one
 * tag, followed down the tree through every level that has it.
 */

import { AppWindow, glide, type Cam, type Rect } from "../AppWindow";
import { ACCENT, Caption, DARK, Pointer, Stage, Statement, keyed, p, s } from "../kit";
import { BASE, nodeId } from "../live/world";
import type { Navigate } from "../live/backend";

export const STRUCTURE = s(28);
const T = DARK;

const K = {
  enter: s(1.9),
  vertical: s(2.8),
  containers: s(4.2),
  ledger: s(5.8),
  horizontal: s(8.0),
  fraud: s(8.4),
  dashboard: s(10.0),
  hex: s(11.6),
  layers: s(13.0),
  ears: s(15.2),
  concerns: s(21.0),
  lens: s(22.2),
};

function navAt(f: number): Navigate {
  const map = (focus: string | null): Navigate => ({ view: "diagram", diagramFocus: focus, lens: null });
  if (f < K.containers) return map(null);
  if (f < K.ledger) return map(nodeId("Aperture Pay"));
  if (f < K.fraud) return map(nodeId("Ledger"));
  if (f < K.dashboard) return map(nodeId("Fraud"));
  if (f < K.hex) return map(nodeId("Dashboard"));
  if (f < K.ears) return map(nodeId("Ledger"));
  return { view: "wiki", page: nodeId("Ledger"), lens: f >= K.lens ? "persistence" : null };
}

const ROWS = ["row0", "row1", "row2", "row3", "row4", "row5", "row6"] as const;
type Name = (typeof ROWS)[number] | "rows" | "lens";

/** The Concern lens picker in the tree, in window pixels (measured). */
const LENS: [number, number] = [164, 159];

export function Structure({ f }: { f: number }) {
  const enter = p(f, K.enter, s(1.0));
  const cam = (at: Partial<Record<Name, Rect>>): Cam => {
    const rows = at.rows;
    const onRows = rows ? { x: rows.cx + 40, y: rows.cy, s: Math.min(1.5, 1280 / (rows.w + 100)) } : { x: 900, y: 450, s: 1.3 };
    return keyed<Cam>(f, { x: 800, y: 500, s: 0.66, rx: 0, ry: 0 }, [
      { at: K.enter, dur: s(1.2), s: 0.84 },
      { at: K.fraud - s(0.6), dur: s(1.0), x: 965, y: 523, s: 1.0 },
      { at: K.layers, dur: s(1.0), x: 200, y: 420, s: 1.75 },
      { at: K.ears, dur: s(1.0), ...onRows },
      { at: K.concerns, dur: s(1.0), x: 200, y: 260, s: 1.6 },
      { at: K.lens + s(0.6), dur: s(4.2), x: 200, y: 760, s: 1.6 },
    ]);
  };
  const targets: Record<Name, { sel: string; all?: boolean; nth?: number }> = {
    rows: { sel: '[id^="resp-"]', all: true },
    lens: { sel: "select" },
    ...Object.fromEntries(ROWS.map((r, i) => [r, { sel: '[id^="resp-"]', nth: i }])),
  } as Record<Name, { sel: string; all?: boolean; nth?: number }>;
  const ptr = glide(f, {}, [
    { at: K.concerns, to: [700, 600] },
    { at: K.lens - s(0.1), to: LENS, click: true, dur: 30 },
  ]);
  return (
    <Stage theme={T}>
      <AppWindow
        f={f}
        theme={T}
        fixture={BASE}
        version={0}
        nav={navAt(f)}
        cam={cam}
        opacity={enter}
        targets={targets}
        overlay={(at) => (
          <>
            {f >= K.ears &&
              f < K.concerns + s(0.4) &&
              ROWS.map((r, i) => {
                const box = at[r];
                if (!box) return null;
                const start = K.ears + s(1.0) + i * s(0.55);
                const t = p(f, start, s(0.6), "move");
                const fade = 1 - p(f, K.concerns - s(0.4), s(0.6));
                return (
                  <div
                    key={r}
                    style={{
                      position: "absolute",
                      left: box.x,
                      top: box.y + 2,
                      width: Math.min(box.w, 780) * t,
                      height: Math.min(box.h, 30),
                      borderRadius: 4,
                      background: "rgba(59,130,246,0.16)",
                      borderBottom: `2px solid ${ACCENT}`,
                      opacity: fade * (t > 0 ? 1 : 0),
                    }}
                  />
                );
              })}
            <Pointer f={f} x={ptr.x} y={ptr.y} clickAt={ptr.clickAt} opacity={p(f, K.concerns, 8) * (1 - p(f, K.lens + s(0.8), 10))} />
          </>
        )}
      />
      <Statement f={f} theme={T} index="03" text="Understand the structure without reading the code." />
      <Caption f={f} theme={T} at={K.vertical} until={K.horizontal - s(0.2)}>
        C4, vertically: the system, its containers, their components.
      </Caption>
      <Caption f={f} theme={T} at={K.horizontal} until={K.ears - s(0.2)}>
        C4, horizontally: every container drawn in its declared style, every component on its layer.
      </Caption>
      <Caption f={f} theme={T} at={K.ears} until={K.concerns - s(0.2)}>
        EARS, horizontally: every claim is one sentence of the same shape, so you scan across them.
      </Caption>
      <Caption f={f} theme={T} at={K.concerns} until={STRUCTURE}>
        Concerns, vertically: one tag, followed down through every level that has it.
      </Caption>
    </Stage>
  );
}
