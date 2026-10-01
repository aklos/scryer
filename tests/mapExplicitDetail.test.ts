/**
 * The map shows extra detail only on an explicit click: a red edge's reasons
 * open on a click of that edge (not on hover, not when a node selection
 * lights it), a layer's description on a click of its ⓘ, and map content
 * carries no hover tooltips at all.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { nextOpenEdge, toggleOpen } from "../src/widgets/diagram-canvas/edgePanel";
import { showsViolationPanel } from "../src/shared/diagram/RelationshipEdge";

describe("red edge reasons", () => {
  const red = { id: "e1", violations: ["domain imports infrastructure"] };

  it("opens on a click of a red edge and closes on a second click", () => {
    expect(nextOpenEdge(null, red)).toBe("e1");
    expect(nextOpenEdge("e1", red)).toBeNull();
  });

  it("moves to another red edge, and closes on a click of a plain one", () => {
    expect(nextOpenEdge("e1", { id: "e2", violations: ["x"] })).toBe("e2");
    expect(nextOpenEdge("e1", { id: "plain" })).toBeNull();
  });

  it("lists reasons only while the edge is the clicked one, never because a selection lights it", () => {
    expect(showsViolationPanel({ violations: ["x"], highlighted: true })).toBe(false);
    expect(showsViolationPanel({ violations: ["x"], open: true })).toBe(true);
    expect(showsViolationPanel({ open: true })).toBe(false);
  });
});

describe("layer descriptions", () => {
  it("toggle open and closed on a click of the layer's ⓘ", () => {
    const once = toggleOpen(new Set(), 2);
    expect([...once]).toEqual([2]);
    expect([...toggleOpen(once, 2)]).toEqual([]);
  });
});

describe("map content", () => {
  it("carries no hover tooltips", () => {
    for (const f of [
      "src/entities/model/DiagramCard.tsx",
      "src/widgets/diagram-canvas/DiagramView.tsx",
      "src/widgets/diagram-canvas/StyleRegions.tsx",
      "src/shared/diagram/RelationshipEdge.tsx",
    ]) {
      const src = readFileSync(f, "utf8");
      // SVG <title> and computed title={…} attributes are hover tooltips; a
      // fixed title="…" on a control button names the control.
      expect(src, f).not.toMatch(/<title>|title=\{/);
    }
  });
});
