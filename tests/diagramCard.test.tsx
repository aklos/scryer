import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ReactFlowProvider } from "@xyflow/react";
import { DiagramCard, type CardData } from "../src/widgets/diagram-canvas/DiagramCard";
import type { DiagramNode } from "../src/widgets/diagram-canvas/diagramLayout";

const node = {
  id: "ui",
  kind: "container",
  name: "Desktop App UI",
  description: "The React app",
  childCount: 6,
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
  it("Names the claim states inside a card in words, the ones needing a look first", () => {
    const html = draw({ claims: { proven: 2, stale: 10, failing: 1, planned: 0 } as never });
    const foot = html.replace(/<[^>]+>/g, "");
    expect(foot).toContain("1 failing · 10 unverified · 2 proven");
    expect(foot).not.toContain("todo");
  });

  it("Shows a styled container's glyph in the corner, clear of the card's text", () => {
    const html = draw({});
    // The glyph sits in its own absolutely placed corner box, not in the
    // flow above the name.
    const corner = html.match(/<div class="absolute right-2\.5 top-2[^"]*" aria-label="Drawn as feature-sliced">\s*<svg/);
    expect(corner).not.toBeNull();
    const name = html.indexOf("Desktop App UI");
    expect(html.slice(html.indexOf("Drawn as"), name).match(/<svg/g)?.length).toBe(1);
  });

  it("When the card is selected, gives the corner to the drill button", () => {
    const html = draw({ selected: true, node: node });
    expect(html).not.toContain("Drawn as");
    expect(html).toContain("Drill into this node");
  });
});
