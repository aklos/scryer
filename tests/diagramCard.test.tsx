import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ReactFlowProvider } from "@xyflow/react";
import { DiagramCard, type CardData } from "../src/widgets/diagram-canvas/DiagramCard";
import type { DiagramNode } from "../src/widgets/diagram-canvas/diagramLayout";
import { childTally } from "../src/entities/model/claimState";

const node = {
  id: "ui",
  kind: "container",
  name: "Desktop App UI",
  description: "The React app",
  childIds: ["app", "entities", "features", "pages", "shared", "widgets"],
  reference: false,
  style: "feature-sliced",
  hasClaims: true,
  x: 0,
  y: 0,
} as unknown as DiagramNode;

const draw = (data: Partial<CardData>) =>
  renderToStaticMarkup(
    <ReactFlowProvider>
      <DiagramCard {...({ id: "ui", data: { node, selected: false, ...data } } as unknown as Parameters<typeof DiagramCard>[0])} />
    </ReactFlowProvider>,
  );

describe("DiagramCard", () => {
  it("Counts a card's direct children on its foot, each once in the most urgent state its claims hold, in the tree's letters", () => {
    const counts = (o: Partial<Record<"proven" | "stale" | "failing" | "planned", number>>) =>
      ({ claims: 0, proven: 0, stale: 0, failing: 0, planned: 0, ...o }) as never;
    // Six children: one with a failing claim among proven ones, two with
    // todos, two all proven, one with no claims at all.
    const t = childTally([
      counts({ failing: 1, proven: 40 }),
      counts({ planned: 3, proven: 9 }),
      counts({ planned: 1 }),
      counts({ proven: 12 }),
      counts({ proven: 2 }),
      undefined,
    ]);
    expect(t).toEqual({ total: 6, byState: { failing: 1, planned: 2, stale: 0, proven: 2 } });
    const html = draw({ children: t });
    expect(html).toContain('aria-label="6 children, 1 failing, 2 todo, 2 proven"');
    expect(html.replace(/<[^>]+>/g, "")).toContain("6·1F2T2P");
    // Hovered, it spells the letters out.
    expect(html).toContain("1F — failing\n2T — todo\n2P — proven\n1 with no claims");
  });

  it("Shows a styled container's glyph in the corner, clear of the card's text", () => {
    const html = draw({});
    // The glyph sits in its own absolutely placed corner box, not in the
    // flow above the name.
    const corner = html.match(/<div class="absolute right-2\.5 top-2[^"]*" aria-label="Drawn as feature-sliced"[^>]*>\s*<svg/);
    expect(corner).not.toBeNull();
    // Hovered, it names the style and what it asks of the code.
    expect(html).toMatch(/title="Architecture style: feature-sliced\n\S/);
    const name = html.indexOf("Desktop App UI");
    expect(html.slice(html.indexOf("Drawn as"), name).match(/<svg/g)?.length).toBe(1);
  });

  it("When a person is drawn on a deeper level, outlines its silhouette hollow like the level's other references, solid (the dash is the external systems'), its name at full contrast", () => {
    const person = { ...node, id: "dev", kind: "person", name: "developer", style: undefined } as unknown as DiagramNode;
    const home = draw({ node: person });
    const away = draw({ node: { ...person, reference: true } as DiagramNode });
    expect(home).toContain("url(#person-fade-ui)");
    expect(home).not.toContain("stroke-dasharray");
    expect(away).not.toContain("url(#person-fade-ui)");
    expect(away).not.toContain("stroke-dasharray");
    expect(away).toContain("stroke-[var(--map-card-stroke)]");
    expect(away).not.toMatch(/opacity-\d/);
    expect(draw({ node: { ...node, reference: true, external: true } as DiagramNode })).not.toMatch(/^<div[^>]*opacity-\d/);
  });

  it("When the card is selected, gives the corner to the drill button", () => {
    const html = draw({ selected: true, node: node });
    expect(html).not.toContain("Drawn as");
    expect(html).toContain("Drill into this node");
  });
});
