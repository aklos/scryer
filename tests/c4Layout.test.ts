import { describe, expect, it } from "vitest";
import { c4Layout, type C4Input } from "../src/widgets/diagram-canvas/c4Layout";

const base: C4Input = {
  inside: ["ui", "core", "db"],
  tooling: [],
  above: [],
  beside: [],
  below: [],
  edges: [
    { source: "ui", target: "core" },
    { source: "core", target: "db" },
  ],
  cardW: 180,
  cardH: 160,
};

type Layout = ReturnType<typeof c4Layout>;
type Pt = { x: number; y: number };

/** Crossings in the drawing: each link from its source's centre through its
 *  lane points to its target's centre, pairs sharing an end skipped. */
function drawnCrossings(l: Layout, edges: { source: string; target: string }[]): number {
  const centre = (id: string) => ({ x: l.positions.get(id)!.x + 90, y: l.positions.get(id)!.y + 80 });
  const legs = edges.map((e) => {
    const p = [centre(e.source), ...(l.routes.get(`${e.source}\0${e.target}`) ?? []), centre(e.target)];
    return p.slice(1).map((q, i) => [p[i], q] as [Pt, Pt]);
  });
  const o = (a: Pt, b: Pt, c: Pt) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
  const cross = ([p, q]: [Pt, Pt], [r, s]: [Pt, Pt]) => o(p, q, r) * o(p, q, s) < 0 && o(r, s, p) * o(r, s, q) < 0;
  let n = 0;
  for (let i = 0; i < edges.length; i++)
    for (let j = i + 1; j < edges.length; j++) {
      const a = edges[i], b = edges[j];
      if ([a.source, a.target].some((v) => v === b.source || v === b.target)) continue;
      for (const g of legs[i]) for (const h of legs[j]) if (cross(g, h)) n++;
    }
  return n;
}

const y = (l: ReturnType<typeof c4Layout>, id: string) => l.positions.get(id)!.y;
const x = (l: ReturnType<typeof c4Layout>, id: string) => l.positions.get(id)!.x;

describe("c4Layout", () => {
  it("Places a node above the nodes it uses", () => {
    const l = c4Layout(base);
    expect(y(l, "ui")).toBeLessThan(y(l, "core"));
    expect(y(l, "core")).toBeLessThan(y(l, "db"));
  });

  it("When the level's links form a cycle, still places every node", () => {
    const l = c4Layout({ ...base, edges: [...base.edges, { source: "db", target: "ui" }] });
    expect(l.positions.size).toBe(3);
  });

  it("Places people above the boundary and external systems beside it", () => {
    const l = c4Layout({
      ...base,
      above: ["dev"],
      beside: ["ext"],
      edges: [...base.edges, { source: "dev", target: "ui" }, { source: "core", target: "ext" }],
    });
    const b = l.boundary!;
    expect(y(l, "dev") + 160).toBeLessThan(b.y);
    expect(x(l, "ext")).toBeGreaterThan(b.x + b.w);
  });

  it("Keeps tooling inside the boundary, in a band below the product", () => {
    const l = c4Layout({ ...base, tooling: ["demo"] });
    const b = l.boundary!, band = l.toolingBand!;
    expect(y(l, "demo")).toBeGreaterThan(y(l, "db"));
    expect(band.y + band.h).toBeLessThanOrEqual(b.y + b.h);
  });

  it("Never overlaps two cards", () => {
    const l = c4Layout({
      ...base,
      inside: ["a", "b", "c", "d", "e", "f", "g", "core"],
      edges: ["a", "b", "c", "d", "e", "f", "g"].map((s) => ({ source: s, target: "core" })),
      above: ["p", "q"],
      beside: ["x1", "x2"],
      tooling: ["t1", "t2"],
    });
    const ps = [...l.positions.values()];
    for (let i = 0; i < ps.length; i++)
      for (let j = i + 1; j < ps.length; j++)
        expect(Math.abs(ps[i].x - ps[j].x) >= 180 || Math.abs(ps[i].y - ps[j].y) >= 160).toBe(true);
  });

  it("Routes a link that spans rows down a lane clear of the cards it passes", () => {
    const l = c4Layout({
      ...base,
      inside: ["ui", "core", "db", "log"],
      edges: [...base.edges, { source: "ui", target: "db" }, { source: "ui", target: "log" }, { source: "core", target: "log" }],
    });
    const route = l.routes.get("ui\0db")!;
    expect(route.length).toBeGreaterThan(0);
    for (const id of ["core", "log"]) {
      const p = l.positions.get(id)!;
      for (const pt of route) expect(pt.x > p.x && pt.x < p.x + 180 && pt.y > p.y && pt.y < p.y + 160).toBe(false);
    }
  });

  it("Routes a tooling link that spans rows down a lane too, so a selection shows it clear of the cards", () => {
    const l = c4Layout({
      ...base,
      tooling: ["demo"],
      edges: [...base.edges, { source: "demo", target: "ui" }],
    });
    const route = l.routes.get("demo\0ui")!;
    expect(route?.length).toBeGreaterThan(0);
    for (const id of ["core", "db"]) {
      const p = l.positions.get(id)!;
      for (const pt of route) expect(pt.x > p.x && pt.x < p.x + 180 && pt.y > p.y && pt.y < p.y + 160).toBe(false);
    }
  });

  it("Centres a card under the one card that uses it", () => {
    const l = c4Layout({
      ...base,
      inside: ["top", "a", "b", "c", "leaf"],
      edges: [
        { source: "top", target: "a" },
        { source: "top", target: "b" },
        { source: "top", target: "c" },
        { source: "c", target: "leaf" },
      ],
    });
    expect(x(l, "leaf")).toBeCloseTo(x(l, "c"), 0);
  });

  it("When a level can be drawn without crossings, draws it without any", () => {
    // One way to assign its rows; ordered by neighbours' mean slot alone,
    // it keeps two crossings.
    const edges = [["a", "e"], ["b", "e"], ["c", "f"], ["c", "g"], ["c", "h"], ["e", "k"], ["f", "i"], ["f", "l"], ["g", "k"], ["h", "j"]].map(
      ([source, target]) => ({ source, target }),
    );
    const l = c4Layout({ ...base, inside: [..."abcefghijkl"], edges });
    expect(drawnCrossings(l, edges)).toBe(0);
  });

  it("If every drawing of a level crosses, takes the fewest crossings", () => {
    // Two cards that each use the same two: one crossing is unavoidable.
    const edges = [["a", "c"], ["a", "d"], ["b", "c"], ["b", "d"], ["c", "e"], ["d", "f"]].map(([source, target]) => ({ source, target }));
    const l = c4Layout({ ...base, inside: [..."abcdef"], edges });
    expect(drawnCrossings(l, edges)).toBe(1);
  });

  it("Routes a person's link to a deep row down a lane clear of the rows between", () => {
    const edges = [...base.edges, { source: "dev", target: "db" }, { source: "dev", target: "ui" }];
    const l = c4Layout({ ...base, inside: ["ui", "core", "db", "log"], above: ["dev"], edges: [...edges, { source: "ui", target: "log" }] });
    const route = l.routes.get("dev\0db")!;
    expect(route.length).toBeGreaterThan(0);
    for (const id of ["ui", "core", "log"]) {
      const p = l.positions.get(id)!;
      for (const pt of route) expect(pt.x > p.x && pt.x < p.x + 180 && pt.y > p.y && pt.y < p.y + 160).toBe(false);
    }
  });

  it("Puts a card that links to an external system at the end of its row nearest the external", () => {
    const l = c4Layout({
      ...base,
      inside: ["top", "a", "b", "c"],
      beside: ["ext"],
      edges: [
        { source: "top", target: "a" },
        { source: "top", target: "b" },
        { source: "top", target: "c" },
        { source: "a", target: "ext" },
      ],
    });
    expect(x(l, "a")).toBeGreaterThan(x(l, "b"));
    expect(x(l, "a")).toBeGreaterThan(x(l, "c"));
  });

  it("Lays out a busy level of twenty cards in well under a second", () => {
    let seed = 11;
    const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
    const ids = Array.from({ length: 20 }, (_, i) => `n${i}`);
    const edges = [];
    for (let i = 0; i < 20; i++) for (let j = i + 1; j < 20; j++) if (rnd() < 0.18) edges.push({ source: ids[i], target: ids[j] });
    // The fastest of three runs: a suite running beside it on a loaded
    // machine can stall any one of them.
    let fastest = Infinity;
    let l = c4Layout({ ...base, inside: [], edges: [] });
    for (let run = 0; run < 3; run++) {
      const t0 = performance.now();
      l = c4Layout({ ...base, inside: ids, edges });
      fastest = Math.min(fastest, performance.now() - t0);
    }
    expect(fastest).toBeLessThan(800);
    expect(l.positions.size).toBe(20);
  });
});
