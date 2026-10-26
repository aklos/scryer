/**
 * The map shows what it means without asking for clicks: a red edge always
 * carries its reason, a layer's description opens on a click of its ⓘ, and
 * the only hover tooltips on it spell out a card's shorthand: its foot's
 * letters and its style glyph.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { toggleOpen } from "../src/widgets/diagram-canvas/edgePanel";
import { edgeLook, edgeText, labelInFull } from "../src/shared/diagram/RelationshipEdge";

describe("red edge reasons", () => {
  it("Writes a red edge's reason on it in full, always, with how many more there are", () => {
    const one = { violations: ["domain imports infrastructure: order.rs → pg.rs"] };
    expect(edgeText(one)).toBe("domain imports infrastructure: order.rs → pg.rs");
    expect(labelInFull(one)).toBe(true);
    expect(edgeLook(one).label).toBe(true);
    expect(edgeText({ label: "uses", violations: ["a", "b", "c"] })).toBe("a (+2 more)");
  });
});

describe("quiet edges", () => {
  it("Draws an edge the drawing already implies faint and thin, with no label until it is lit", () => {
    const plain = edgeLook({ label: "uses" });
    const quiet = edgeLook({ label: "uses", quiet: true });
    expect(quiet.opacity).toBeLessThan(plain.opacity);
    expect(quiet.width).toBeLessThan(plain.width);
    expect(quiet.label).toBe(false);
    expect(plain.label).toBe(true);
  });

  it("When an end is selected, lights a quiet edge in full like any other", () => {
    const lit = edgeLook({ label: "uses", quiet: true, highlighted: true });
    expect(lit).toEqual(edgeLook({ label: "uses", highlighted: true }));
    expect(lit.label).toBe(true);
  });
});

describe("edge labels", () => {
  it("While no end of an edge is selected, its label recedes; when one is, reads in full", () => {
    expect(labelInFull({ label: "calls" })).toBe(false);
    expect(labelInFull({ label: "calls", highlighted: true })).toBe(true);
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
  it("Carries no hover tooltips beyond the two that spell out a card's shorthand", () => {
    const card = readFileSync("src/widgets/diagram-canvas/DiagramCard.tsx", "utf8");
    expect(card.match(/title=\{/g)).toEqual(["title={", "title={"]);
    expect(card).toMatch(/title=\{hover\}/);
    expect(card).toMatch(/title=\{styleHover\(node\.style\)\}/);
    for (const f of [
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
