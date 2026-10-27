import { describe, expect, it } from "vitest";
import { edgeShape } from "../src/shared/diagram/edgeGeometry";

describe("edgeShape", () => {
  it("Runs a link through its turns in straight stretches, each turn rounded", () => {
    // Out of a card's foot, across a gap, down a lane, into a card's head.
    const s = edgeShape(0, 160, 140, 660, {
      waypoints: [
        { x: 0, y: 205 },
        { x: 140, y: 205 },
      ],
    });
    expect(s.path).not.toMatch(/ C /);
    expect(s.path.match(/ Q /g)).toHaveLength(2);
    // The arrow points straight down into the target.
    expect(s.arrowAngle).toBeCloseTo(Math.PI / 2);
    expect(s.points).toEqual([{ x: 0, y: 160 }, { x: 0, y: 205 }, { x: 140, y: 205 }, { x: 140, y: 660 }]);
  });

  it("Draws a link with no turns as one straight line", () => {
    const s = edgeShape(90, 160, 90, 250);
    expect(s.path).toBe("M 90 160 L 90 250");
  });

  it("Ends the drawn line where the arrowhead begins, back along the way it arrives, with the line's ends and points whole", () => {
    expect(edgeShape(90, 160, 90, 250, { trimEnd: 7 }).path).toBe("M 90 160 L 90 243");
    // A routed line that passes behind its target on the way in stays whole
    // there: only its last stretch is shortened.
    const s = edgeShape(0, 0, 100, 100, { waypoints: [{ x: 0, y: 200 }, { x: 100, y: 200 }], trimEnd: 7 });
    expect(s.path).toMatch(/L 100 107$/);
    expect(s.path).toMatch(/200/);
    expect(s.points[s.points.length - 1]).toEqual({ x: 100, y: 100 });
    expect([s.tx, s.ty]).toEqual([100, 100]);
  });
});
