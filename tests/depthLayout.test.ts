/** Todo claims for `depthLayout`. */
import { describe, expect, it } from "vitest";
import { depthLayout } from "../src/widgets/diagram-canvas/depth";

describe("depthLayout", () => {
  it("When a band holds more symbols than the level is wide, wraps it into lines offset by half a step, so a few symbols with many dependencies stay readable", () => {
    // The App level: one root symbol calling thirty symbols elsewhere.
    const deps = Array.from({ length: 30 }, (_, i) => `dep${i}`);
    const { centers, regions } = depthLayout(
      [{ id: "App", size: 20, labelW: 40 }],
      deps,
      deps.map((d) => ({ source: "App", target: d })),
    );
    const xs = deps.map((d) => centers.get(d)!.x), ys = deps.map((d) => centers.get(d)!.y);
    const w = Math.max(...xs) - Math.min(...xs), h = Math.max(...ys) - Math.min(...ys) + 140;
    expect(new Set(ys).size).toBeGreaterThan(1);
    expect(w / h).toBeLessThan(4);
    // Neighbouring lines stagger: no dot sits straight below the one above.
    const lineOf = new Map<number, number[]>();
    for (let i = 0; i < 30; i++) lineOf.set(ys[i], [...(lineOf.get(ys[i]) ?? []), xs[i]]);
    const [first, second] = [...lineOf.keys()].sort((a, b) => a - b).map((y) => lineOf.get(y)!);
    for (const x of second) expect(first.some((f) => Math.abs(f - x) < 1)).toBe(false);
    // The band's region wraps every line.
    const band = regions.find((r) => r.layer === "depends on")!;
    expect(band.shape === "rect" && band.y + band.h).toBeGreaterThan(Math.max(...ys));
  });
  it.todo("Places a component's exposed symbols on the top row, each other symbol one row below its nearest caller, and symbols nothing reaches on a row past the deepest");
});
