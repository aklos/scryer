import { describe, expect, it } from "vitest";
import { assignPorts, orthoRoutes, portPoint } from "../src/widgets/diagram-canvas/edgePorts";

const card = (id: string, x: number, y: number) => ({ id, x, y, w: 180, h: 160 });

type P = { x: number; y: number };
/** Proper crossings between two paths of straight stretches. */
function crossings(a: P[], b: P[]): number {
  const o = (p: P, q: P, r: P) => Math.sign((q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x));
  let n = 0;
  for (let i = 1; i < a.length; i++)
    for (let j = 1; j < b.length; j++) {
      const [p, q, r, s] = [a[i - 1], a[i], b[j - 1], b[j]];
      if (o(p, q, r) * o(p, q, s) < 0 && o(r, s, p) * o(r, s, q) < 0) n++;
    }
  return n;
}

describe("assignPorts", () => {
  it("Joins cards in two rows foot to head, and a card to the column beside side to side", () => {
    const boxes = [card("user", 0, 0), card("app", 0, 250), card("ext", 500, 400)];
    const ports = assignPorts(
      boxes,
      [
        { id: "uses", source: "user", target: "app" },
        { id: "calls", source: "app", target: "ext" },
      ],
      "down",
      new Set(["ext"]),
    );
    expect(ports.get("uses")).toMatchObject({ sourceSide: "bottom", targetSide: "top", flow: "down" });
    // Two rows apart vertically, but the column beside is reached sideways.
    expect(ports.get("calls")).toMatchObject({ sourceSide: "right", targetSide: "left", flow: "across" });
  });

  it("When a link climbs, leaves its source's head and enters its target's foot", () => {
    const ports = assignPorts([card("a", 0, 0), card("b", 0, 250)], [{ id: "up", source: "b", target: "a" }], "down");
    expect(ports.get("up")).toMatchObject({ sourceSide: "top", targetSide: "bottom" });
  });

  it("Spreads the edges on one side of a card, each toward where it heads, so none crosses another there", () => {
    const boxes = [card("hub", 250, 0), card("l", 0, 250), card("m", 250, 250), card("r", 500, 250)];
    const ports = assignPorts(
      boxes,
      ["r", "l", "m"].map((t) => ({ id: t, source: "hub", target: t })),
      "down",
    );
    const at = (id: string) => portPoint(boxes[0], "bottom", ports.get(id)!.sourceShift).x;
    expect(at("l")).toBeLessThan(at("m"));
    expect(at("m")).toBeLessThan(at("r"));
    // All on the card's foot.
    for (const id of ["l", "m", "r"]) expect(Math.abs(at(id) - 340)).toBeLessThan(90);
  });

  it("Spaces the ports on one side of a card an even step apart, however many share it", () => {
    const steps = (n: number) => {
      const targets = Array.from({ length: n }, (_, i) => card(`t${i}`, i * 240 - 240, 250));
      const ports = assignPorts([card("hub", 0, 0), ...targets], targets.map((t) => ({ id: t.id, source: "hub", target: t.id })), "down");
      const xs = targets.map((t) => ports.get(t.id)!.sourceShift).sort((a, b) => a - b);
      return xs.slice(1).map((x, i) => x - xs[i]);
    };
    expect(new Set([...steps(2), ...steps(3), ...steps(4)])).toEqual(new Set([16]));
  });

  it("When an edge's two ends nearly line up, slides a port along its side so the edge runs straight", () => {
    // A card a little right of the one above it: one edge, no lane.
    const boxes = [card("a", 0, 0), card("b", 30, 250)];
    const p = assignPorts(boxes, [{ id: "ab", source: "a", target: "b" }], "down").get("ab")!;
    expect(portPoint(boxes[0], p.sourceSide, p.sourceShift).x).toBe(portPoint(boxes[1], p.targetSide, p.targetShift).x);
  });

  it("Parts a reverse pair onto two ports that run side by side, never one line on the other", () => {
    const boxes = [card("a", 0, 0), card("b", 500, 0)];
    const ports = assignPorts(
      boxes,
      [
        { id: "ab", source: "a", target: "b" },
        { id: "ba", source: "b", target: "a" },
      ],
      "down",
    );
    const ab = ports.get("ab")!, ba = ports.get("ba")!;
    // At each card the two take different spots, in the same order at both.
    expect(ab.sourceShift).not.toBe(ba.targetShift);
    expect(Math.sign(ab.sourceShift - ba.targetShift)).toBe(Math.sign(ab.targetShift - ba.sourceShift));
  });
});

describe("orthoRoutes", () => {
  it("Turns a link between two offset rows once in the gap, and leaves an aligned one straight", () => {
    const routes = orthoRoutes([
      { id: "jog", start: { x: 90, y: 160 }, end: { x: 330, y: 250 }, axis: "down" },
      { id: "straight", start: { x: 600, y: 160 }, end: { x: 600, y: 250 }, axis: "down" },
    ]);
    expect(routes.get("jog")).toEqual([{ x: 90, y: 205 }, { x: 330, y: 205 }]);
    expect(routes.get("straight")).toEqual([]);
  });

  it("Gives links that turn in one gap tracks of their own, so none runs along another", () => {
    const routes = orthoRoutes([
      { id: "a", start: { x: 0, y: 160 }, end: { x: 300, y: 250 }, axis: "down" },
      { id: "b", start: { x: 100, y: 160 }, end: { x: 400, y: 250 }, axis: "down" },
      { id: "c", start: { x: 600, y: 160 }, end: { x: 700, y: 250 }, axis: "down" },
    ]);
    const y = (id: string) => routes.get(id)![0].y;
    expect(y("a")).not.toBe(y("b"));
    // A stretch clear of the others turns in the gap's middle.
    expect(y("c")).toBe(205);
    for (const id of ["a", "b", "c"]) expect(y(id) > 160 && y(id) < 250).toBe(true);
  });

  it("Never crosses two parallel links that turn in one gap, whichever way they head", () => {
    // Two cards' right sides, two links each way to a card beside: the pair
    // heading down-right and the pair heading up-right.
    for (const [from, to] of [[[440, 470], [800, 826]], [[440, 470], [100, 126]]]) {
      const routes = orthoRoutes(
        from.map((y, i) => ({ id: `e${i}`, start: { x: 410, y }, end: { x: 810, y: to[i] }, axis: "across" as const })),
      );
      const path = (id: string, i: number) => [{ x: 410, y: from[i] }, ...routes.get(id)!, { x: 810, y: to[i] }];
      expect(crossings(path("e0", 0), path("e1", 1))).toBe(0);
    }
  });

  it("Runs a lane's link straight down the lane, turning only to reach it and to leave it", () => {
    const routes = orthoRoutes([
      {
        id: "lane",
        start: { x: 90, y: 160 },
        end: { x: 90, y: 660 },
        axis: "down",
        waypoints: [
          { x: 230, y: 250 },
          { x: 230, y: 410 },
        ],
      },
    ]);
    expect(routes.get("lane")!.map((p) => p.x)).toEqual([90, 230, 230, 90]);
  });
});
