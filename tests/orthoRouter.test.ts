import { describe, expect, it } from "vitest";
import { routeAround } from "../src/widgets/diagram-canvas/orthoRouter";
import { portPoint, type PortBox } from "../src/widgets/diagram-canvas/edgePorts";
import { layoutQuality } from "../src/widgets/diagram-canvas/layoutQuality";

const card = (id: string, x: number, y: number): PortBox => ({ id, x, y, w: 180, h: 160 });

/** Each routed edge as the line it draws: port, turns, port. */
function drawn(boxes: PortBox[], edges: { id: string; source: string; target: string }[]) {
  const boxOf = new Map(boxes.map((b) => [b.id, b]));
  const routes = routeAround(boxes, edges);
  return edges.map((e) => {
    const r = routes.get(e.id)!;
    return {
      ...e,
      route: r,
      points: [
        portPoint(boxOf.get(e.source)!, r.sourceSide, r.sourceShift),
        ...r.waypoints,
        portPoint(boxOf.get(e.target)!, r.targetSide, r.targetShift),
      ],
    };
  });
}

describe("routeAround", () => {
  it("Routes an edge around a card that sits between its two ends", () => {
    const boxes = [card("a", 0, 0), card("m", 300, 0), card("b", 600, 0)];
    const lines = drawn(boxes, [{ id: "e", source: "a", target: "b" }]);
    expect(layoutQuality(boxes, lines).throughCards).toEqual([]);
  });

  it("Leaves and enters each card from a side, in straight runs with right-angle turns", () => {
    const boxes = [card("a", 0, 0), card("b", 400, 300), card("c", 0, 600)];
    const lines = drawn(boxes, [
      { id: "ab", source: "a", target: "b" },
      { id: "cb", source: "c", target: "b" },
    ]);
    for (const l of lines)
      for (let i = 1; i < l.points.length; i++) {
        const p = l.points[i - 1], q = l.points[i];
        expect(p.x === q.x || p.y === q.y).toBe(true);
      }
  });

  it("When several edges meet one side of a card, spreads their ports along it, so a reverse pair draws as two lines", () => {
    const boxes = [card("a", 0, 0), card("b", 0, 400)];
    const lines = drawn(boxes, [
      { id: "ab", source: "a", target: "b" },
      { id: "ba", source: "b", target: "a" },
    ]);
    const [ab, ba] = lines.map((l) => l.route);
    expect(ab.sourceSide).toBe(ba.targetSide);
    expect(ab.sourceShift).not.toBe(ba.targetShift);
    expect(layoutQuality(boxes, lines).sharedRuns).toBe(0);
  });

  it("Parts edges that would run along one line onto tracks of their own", () => {
    // Two cards on the left each calling one on the right past a card between.
    const boxes = [card("a", 0, 0), card("b", 0, 400), card("m", 400, 200), card("x", 800, 0), card("y", 800, 400)];
    const lines = drawn(boxes, [
      { id: "ay", source: "a", target: "y" },
      { id: "bx", source: "b", target: "x" },
    ]);
    const q = layoutQuality(boxes, lines);
    expect(q.throughCards).toEqual([]);
    expect(q.sharedRuns).toBe(0);
  });

  it("Keeps every route's first and last stretch longer than an arrowhead, however its tracks are parted", () => {
    // Two rows 50px apart: the runs that share the line between them were
    // parted to 8px from the card one of them turns into.
    const boxes = [card("c0", 0, 630), card("c2", 920, 210), card("c3", 690, 630), card("c4", 1150, 0), card("c5", 0, 840), card("c6", 460, 630), card("c7", 1150, 210)];
    // Three links c0 → c3 (one per import pair) bunch on the line under c0.
    const edges = [["c6", "c5"], ["c0", "c3"], ["c4", "c2"], ["c2", "c6"], ["c7", "c3"], ["c0", "c3"], ["c0", "c5"], ["c0", "c3"]].map(([source, target], i) => ({ id: `e${i}`, source, target }));
    for (const l of drawn(boxes, edges)) {
      const n = l.points.length;
      const leg = (p: { x: number; y: number }, q: { x: number; y: number }) => Math.hypot(q.x - p.x, q.y - p.y);
      expect(leg(l.points[0], l.points[1])).toBeGreaterThanOrEqual(16);
      expect(leg(l.points[n - 2], l.points[n - 1])).toBeGreaterThanOrEqual(16);
    }
  });

  it("If no route clears the cards, then leaves the edge unrouted", () => {
    // A card walled in by four others, closer than a route's clearance.
    const boxes = [card("t", 300, 300), card("n", 300, 130), card("s", 300, 470), card("w", 110, 300), card("e", 490, 300), card("a", 900, 900)];
    expect(routeAround(boxes, [{ id: "e1", source: "a", target: "t" }]).has("e1")).toBe(false);
  });
});
