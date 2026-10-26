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
  it("When nothing is in the way, puts a label at the middle of its own line", () => {
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label: "calls" }], []).get("a")!;
    expect(at).toEqual({ x: 200, y: 0 });
  });

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

  it("Keeps a label off the cards", () => {
    const card = { x: 150, y: -40, w: 100, h: 80 };
    const label = "stores entries in";
    const at = placeLabels([{ id: "a", points: [{ x: 0, y: 0 }, { x: 400, y: 0 }], label }], [card]).get("a")!;
    const r = rectOf(at, label);
    const clear = r.x + r.w <= card.x || card.x + card.w <= r.x || r.y + r.h <= card.y || card.y + card.h <= r.y;
    expect(clear).toBe(true);
  });
});
