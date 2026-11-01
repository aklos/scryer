/** Claims for the Activity page and its feed. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DiffView, listedNodes } from "../src/pages/landing/ActivityPage";
import { diffTally, type ModelDiff } from "../src/features/activity/useActivity";

const diff: ModelDiff = {
  nodes: [
    {
      id: "notes",
      name: "notes",
      kind: "component",
      path: ["Notes", "App"],
      change: "modified",
      notes: ["now uses tags"],
      claims: [
        { id: "c1", statement: "Saves a draft", was: "Saves a note", change: "modified" },
        { id: "c2", statement: "Shares a note", change: "added" },
        { id: "c3", statement: "Prints a note", change: "removed" },
      ],
    },
    { id: "old", name: "old", kind: "component", path: ["Notes", "App"], change: "removed", claims: [] },
    { id: "fmt", name: "fmt", kind: "symbol", path: ["Notes", "App", "notes"], change: "added", claims: [] },
  ],
};

describe("ActivityPage", () => {
  it("Tallies a commit's claims added, reworded and removed, and the nodes other than code symbols it changed other than through their claims", () => {
    expect(diffTally(diff)).toEqual({ added: 1, reworded: 1, removed: 1, nodes: 2 });
  });

  it("Lists a code symbol only when its claims changed, counting the rest", () => {
    const { nodes, symbols } = listedNodes(diff);
    expect(nodes.map((n) => n.id)).toEqual(["notes", "old"]);
    expect(symbols).toBe(1);
  });

  it("Shows a diff node by node, linking what is still in the model, word-diffing a reworded claim and striking what was removed", () => {
    const html = renderToStaticMarkup(
      <DiffView
        diff={diff}
        links={{
          node: (id) => (id === "notes" ? () => {} : undefined),
          claim: (id) => (id === "c1" || id === "c2" ? () => {} : undefined),
        }}
      />,
    );
    expect(html).toContain("Notes / App / ");
    expect(html).toMatch(/<button[^>]*>notes<\/button>/);
    expect(html).toContain("now uses tags");
    expect(html).toMatch(/line-through[^>]*>old</);
    expect(html).toMatch(/line-through[^>]*>Prints a note</);
    expect(html).toContain("draft");
    expect(html).toContain("note");
    expect(html).toContain("and 1 code symbol changed without a claim");
  });
});
