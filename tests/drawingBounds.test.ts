import { describe, expect, it } from "vitest";
import { drawingBounds } from "../src/widgets/diagram-canvas/StyleRegions";

describe("fitting the map", () => {
  it("Frames the whole drawing when it fits the view: the cards and the regions drawn behind them", () => {
    const cards = { x: 0, y: 0, width: 200, height: 100 };
    // A ring reaching above and left of the cards, a band below them.
    const b = drawingBounds(cards, [
      { layer: "shell", shape: "ring", cx: 100, cy: 50, r: 300 },
      { layer: "depends on", shape: "rect", x: -50, y: 400, w: 100, h: 80, ghost: true },
    ]);
    expect(b.x).toBeLessThanOrEqual(100 - 300);
    expect(b.y).toBeLessThanOrEqual(50 - 300);
    expect(b.x + b.width).toBeGreaterThanOrEqual(100 + 300);
    expect(b.y + b.height).toBeGreaterThanOrEqual(480);
    // No regions: just the cards.
    expect(drawingBounds(cards, [])).toEqual(cards);
  });
});
