/**
 * The map shows what it means without asking for clicks: a red edge always
 * carries its reason, a layer's description opens on a click of its ⓘ, and
 * the only hover tooltips on it spell out a card's shorthand: its foot's
 * letters and its style glyph.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { LAYER_NOTE, clickedOff, toggleOpen } from "../src/widgets/diagram-canvas/edgePanel";
import { edgeLook, edgeText, labelInFull } from "../src/shared/diagram/RelationshipEdge";
import { edgeShown, subgraphHighlight, toggleLit } from "../src/widgets/diagram-canvas/DiagramView";

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
  it("Draws an edge that conforms to the style as round beads in a fainter tone with an open arrowhead, still, with no label until it is lit", () => {
    const plain = edgeLook({ label: "uses" });
    const quiet = edgeLook({ label: "uses", quiet: true });
    expect(quiet.dash).toBe("0 4.5");
    expect(plain.dash).toBe("6 3");
    expect([quiet.tone, quiet.arrow]).toEqual(["quiet", "open"]);
    expect([plain.tone, plain.arrow]).toEqual(["plain", "filled"]);
    expect(quiet.flowing).toBe(false);
    expect(plain.flowing).toBe(true);
    expect(quiet.label).toBe(false);
    expect(plain.label).toBe(true);
  });

  it("While the conforming-links control is off, hides an edge that conforms to the style until a selection reaches one of its ends", () => {
    expect(edgeShown(true, true, false)).toBe(true);
    expect(edgeShown(true, false, false)).toBe(false);
    expect(edgeShown(true, false, true)).toBe(true);
    // An edge that says something new always shows.
    expect(edgeShown(false, false, false)).toBe(true);
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

describe("clicked edges", () => {
  it("When an edge or its label is clicked, lights the edge in its own colour with its label in full; a second click on it, or a click on a card or the canvas, puts it back", () => {
    expect(toggleLit(null, "e1")).toBe("e1");
    expect(toggleLit("e1", "e2")).toBe("e2");
    expect(toggleLit("e1", "e1")).toBe(null);
    // Lit like an edge at a selected card, in its own colour, never the
    // selection's: even a crowded or quiet edge reads in full.
    const lit = { label: "calls", labelCrowded: true, quiet: true, highlighted: true };
    expect(labelInFull(lit)).toBe(true);
    expect(edgeLook(lit)).toMatchObject({ label: true, mark: false, tone: "plain", opacity: 1 });
    const view = readFileSync(new URL("../src/widgets/diagram-canvas/DiagramView.tsx", import.meta.url), "utf8");
    expect(view).toContain("onEdgeClick={(_, e) => lightEdge(e.id)}");
    expect(view).not.toContain("selected: e.id === litEdge");
    expect(view).toContain("onLabelClick: () => lightEdge(e.id)");
    expect(view.match(/setLitEdge\(null\);/g)?.length).toBe(2);
    const edge = readFileSync(new URL("../src/shared/diagram/RelationshipEdge.tsx", import.meta.url), "utf8");
    expect(edge).toContain("data?.onLabelClick?.()");
  });
});

describe("lit edges", () => {
  it("While an edge is lit by a click, dims every other edge and every card but its two ends, as selecting a card does", () => {
    const scene = {
      nodes: ["a", "b", "c"].map((id) => ({ id })),
      edges: [
        { id: "ab", source: "a", target: "b" },
        { id: "bc", source: "b", target: "c" },
      ],
    } as unknown as Parameters<typeof subgraphHighlight>[0];
    expect(subgraphHighlight(scene, null, "ab")).toEqual({ active: true, neighbors: new Set(["a", "b"]), edge: "ab" });
    // A lit edge goes ahead of the card selected.
    expect(subgraphHighlight(scene, "c", "ab").edge).toBe("ab");
    expect(subgraphHighlight(scene, "b", null)).toEqual({ active: true, neighbors: new Set(["a", "b", "c"]) });
    expect(subgraphHighlight(scene, null, null).active).toBe(false);
  });
});

describe("label clutter", () => {
  it("While a crowded label's edge is not lit, draws a small mark in its place; when lit, its words", () => {
    const rest = edgeLook({ label: "calls", labelCrowded: true });
    expect(rest).toMatchObject({ label: true, mark: true });
    expect(edgeLook({ label: "calls", labelCrowded: true, highlighted: true }).mark).toBe(false);
    expect(edgeLook({ label: "calls" }).mark).toBe(false);
    // A red edge's reason is never held back.
    expect(edgeLook({ violations: ["x"], labelCrowded: true }).mark).toBe(false);
  });

  it("At rest, outlines a label's letters in the canvas colour instead of boxing them, so any line under it, its own, another edge or a boundary, shows between the words", () => {
    const edge = readFileSync(new URL("../src/shared/diagram/RelationshipEdge.tsx", import.meta.url), "utf8");
    const css = readFileSync(new URL("../src/app/index.css", import.meta.url), "utf8");
    expect(edge).toContain('"edge-label-halo border-transparent text-[var(--text-muted)]"');
    expect(edge).not.toContain("border-transparent bg-[var(--surface-canvas)]");
    expect(css).toMatch(/\.edge-label-halo\s*\{\s*text-shadow:[^}]*var\(--surface-canvas\)/);
  });
});

describe("layer descriptions", () => {
  it("toggle open and closed on a click of the layer's ⓘ", () => {
    const once = toggleOpen(new Set(), 2);
    expect([...once]).toEqual([2]);
    expect([...toggleOpen(once, 2)]).toEqual([]);
  });

  it("When another layer's ⓘ is clicked, opens its description and closes the one open", () => {
    expect([...toggleOpen(new Set([2]), 3)]).toEqual([3]);
  });

  it("When the user presses anywhere off an open description and its ⓘ, closes it", () => {
    const at = (inNote: boolean) => ({ closest: (sel: string) => (inNote && sel === `[${LAYER_NOTE}]` ? {} : null) });
    expect(clickedOff(at(false))).toBe(true);
    expect(clickedOff(null)).toBe(true);
    expect(clickedOff(at(true))).toBe(false);
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
