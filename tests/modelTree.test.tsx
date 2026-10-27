/** Todo claims for `ModelTree`. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClaimMix, levelTitle, type ModelTree } from "../src/widgets/model-tree/ModelTree";
import type { DerivedCounts } from "../src/entities/model/derived";

const counts = (c: Partial<DerivedCounts>): DerivedCounts => ({
  claims: 0, proven: 0, stale: 0, failing: 0, planned: 0,
  untestedComponents: 0, survivingMutants: 0, styleViolations: 0, darkFiles: 0, ...c,
});
const segments = (html: string) => [...html.matchAll(/var\(--claim-mix-(\w+)\)/g)].map((m) => m[1]);

describe("ModelTree", () => {
  it.todo("When the user types in the tree's filter, shows only rows whose name matches and their ancestors, with every surviving branch open");
  it.todo("Shows beside a collapsed row the number of todo claims in its subtree, and beside an open row only its own");

  it("Titles each level of the tree with what its rows are, in the words the map titles its levels with", () => {
    expect(levelTitle(["person", "system", "system"])).toBe("System context");
    expect(levelTitle(["container", "container"])).toBe("Containers");
    expect(levelTitle(["component", "symbol"])).toBe("Components");
    expect(levelTitle(["symbol", "symbol"])).toBe("Code");
  });

  it("Draws a subtree's claims as one bar: failing first, then unverified, todo and proven, each state present at least a sliver", () => {
    const html = renderToStaticMarkup(<ClaimMix counts={counts({ proven: 206, stale: 2, planned: 3 })} empty />);
    expect(segments(html)).toEqual(["stale", "planned", "proven"]);
    // Two changed claims among two hundred still get a visible share.
    const grow = [...html.matchAll(/flex-grow:\s*([\d.]+)/g)].map((m) => Number(m[1]));
    expect(grow[0] / grow.reduce((t, g) => t + g, 0)).toBeGreaterThan(0.05);
    expect(html).toContain("206 proven");
  });

  it("Draws the bar's bare track for code with no claims, and nothing for what holds none by nature", () => {
    const bare = renderToStaticMarkup(<ClaimMix counts={counts({})} empty />);
    expect(bare).toContain("var(--claim-mix-empty)");
    expect(segments(bare)).toEqual(["empty"]);
    const none = renderToStaticMarkup(<ClaimMix counts={undefined} empty={false} />);
    expect(none).not.toContain("claim-mix");
  });
});
