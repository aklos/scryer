import { describe, expect, it } from "vitest";
import { labelSize, placeLabels } from "../src/shared/diagram/labelPlacement";

const rectOf = (at: { x: number; y: number }, label: string) => {
  const s = labelSize(label);
  return { x: at.x - s.w / 2, y: at.y - s.h / 2, w: s.w, h: s.h };
};
/** Whether the horizontal or vertical segment pq passes through r. */
const hits = (p: { x: number; y: number }, q: { x: number; y: number }, r: { x: number; y: number; w: number; h: number }) =>
  Math.max(p.x, q.x) >= r.x && Math.min(p.x, q.x) <= r.x + r.w && Math.max(p.y, q.y) >= r.y && Math.min(p.y, q.y) <= r.y + r.h;

describe("placeLabels", () => {
  it("Moves a label off another edge it would hide", () => {
    // Two lines crossing at their middles: the label must not cover the other.
    const across = [{ x: 200, y: -200 }, { x: 200, y: 200 }];
    const label = "reads claims, their status and health in";
    const at = placeLabels(
      [
        { id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label },
        { id: "b", points: across },
      ],
      [],
    ).get("a")!;
    expect(hits(across[0], across[1], rectOf(at, label))).toBe(false);
  });

  it("Never sets two labels on one another when there is room", () => {
    // A reverse pair five pixels apart, both labelled.
    const l1 = "launches as its MCP server and session hook";
    const l2 = "drives over ACP to write preview fixtures";
    const spots = placeLabels(
      [
        { id: "a", points: [{ x: 0, y: 0 }, { x: 500, y: 0 }], label: l1, pair: "p" },
        { id: "b", points: [{ x: 500, y: 5 }, { x: 0, y: 5 }], label: l2, pair: "p" },
      ],
      [],
    );
    const a = rectOf(spots.get("a")!, l1), b = rectOf(spots.get("b")!, l2);
    const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
    expect(apart).toBe(true);
  });

  it("When a spot beside its line is free, sets a label there, at the line's middle, rather than on the line", () => {
    const label = "calls";
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }], []).get("a")!;
    const r = rectOf(at, label);
    expect(at.x).toBe(200);
    expect(r.y + r.h <= 0 || r.y >= 0).toBe(true);
    expect(at).toMatchObject({ onLine: false, crowded: false });
  });

  it("Beside an upright run, sets the label's near edge against the line, its words starting there", () => {
    const label = "launches as its MCP server and session hook";
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 0, y: 400 }], label }], []).get("a")!;
    expect(at.hug).toBeDefined();
    expect(Math.abs(at.hug!.x)).toBe(4);
    expect(at.hug!.side).toBe(at.hug!.x > 0 ? "left" : "right");
  });

  it("Never sets a label further from its line than just clear of it", () => {
    // Fenced in on both sides: the label stays at its line, crowded, rather
    // than drifting a label's width out to be nobody's.
    const label = "runs with node as its preview sidecar";
    const fences = [10, 20, 30, 45, 60].flatMap((y) => [y, -y]).map((y) => ({ id: `f${y}`, points: [{ x: -100, y }, { x: 500, y }] }));
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }, ...fences], []).get("a")!;
    const s = labelSize(label);
    expect(Math.abs(at.y)).toBeLessThanOrEqual(s.h / 2 + 4);
  });

  it("If nothing beside its line is free, then sets a label on the line and says so", () => {
    // Edges packed along both sides of the line, two label-clearances deep.
    const label = "calls";
    const fence = (y: number) => ({ id: `f${y}`, points: [{ x: -100, y }, { x: 500, y }] });
    const spots = placeLabels(
      [{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }, ...[11, 14, 17, 20, 25, 30, 35, 40].flatMap((y) => [fence(y), fence(-y)])],
      [],
    );
    expect(spots.get("a")!.onLine).toBe(true);
  });

  it("If every spot would cover another edge, a card or a label, then marks the label crowded", () => {
    const label = "reads claims, their status and health in";
    const lines = [-40, -30, -20, -10, 10, 20, 30, 40].map((y) => ({ id: `f${y}`, points: [{ x: -100, y }, { x: 500, y }] }));
    const crossing = [0, 100, 200, 300, 400].map((x) => ({ id: `c${x}`, points: [{ x, y: -100 }, { x, y: 100 }] }));
    const spots = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }, ...lines, ...crossing], []);
    expect(spots.get("a")!.crowded).toBe(true);
    const free = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }], []);
    expect(free.get("a")!.crowded).toBe(false);
  });

  it("Keeps a label off the cards", () => {
    const card = { x: 150, y: -40, w: 100, h: 80 };
    const label = "stores entries in";
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }], [card]).get("a")!;
    const r = rectOf(at, label);
    const clear = r.x + r.w <= card.x || card.x + card.w <= r.x || r.y + r.h <= card.y || card.y + card.h <= r.y;
    expect(clear).toBe(true);
  });
});
